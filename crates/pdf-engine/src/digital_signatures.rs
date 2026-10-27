//! Incremental PDF signing. Private keys are imported into an ephemeral Windows store.
use mupdf::pdf::PdfDocument;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};
use viewer_core::FieldRef;
type Result<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
const MAX_PDF: usize = 256 * 1024 * 1024;
const RESERVATION: usize = 32 * 1024;

pub struct SecretPassword(Vec<u16>);
impl SecretPassword {
    pub fn new(mut value: String) -> Self {
        let mut wide = value.encode_utf16().collect::<Vec<_>>();
        wide.push(0);
        unsafe {
            for byte in value.as_mut_vec() {
                std::ptr::write_volatile(byte, 0);
            }
        }
        Self(wide)
    }
}
impl Drop for SecretPassword {
    fn drop(&mut self) {
        for ch in &mut self.0 {
            unsafe {
                std::ptr::write_volatile(ch, 0);
            }
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}
pub struct SigningRequest {
    pub field: FieldRef,
    pub certificate: PathBuf,
    pub target: PathBuf,
    pub password: SecretPassword,
}
#[derive(Clone, Debug)]
pub struct SignatureIntegrity {
    pub signer: String,
    pub integrity_verified: bool,
    pub certificate_trust_evaluated: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SourceStamp {
    length: u64,
    modified: SystemTime,
    digest: [u8; 32],
}
impl SourceStamp {
    pub fn read(path: &Path) -> Result<Self> {
        let m = fs::metadata(path).map_err(err)?;
        if !m.is_file() {
            return Err("Signing requires a regular PDF file.".into());
        }
        if m.len() > MAX_PDF as u64 {
            return Err("The file exceeds the signing size limit.".into());
        }
        let bytes = read_bounded(path, MAX_PDF)?;
        let digest = platform::digest(&bytes)?;
        let after = fs::metadata(path).map_err(err)?;
        if m.len() != after.len() || m.modified().map_err(err)? != after.modified().map_err(err)? {
            return Err("The source changed during inspection.".into());
        }
        Ok(Self {
            length: m.len(),
            modified: m.modified().map_err(err)?,
            digest,
        })
    }
}
struct Staged(PathBuf);
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn read_bounded(path: &Path, max: usize) -> Result<Vec<u8>> {
    let mut bytes = vec![];
    File::open(path)
        .map_err(err)?
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() > max {
        return Err("The file exceeds the signing size limit.".into());
    }
    Ok(bytes)
}
struct PrivateBytes(Vec<u8>);
impl Drop for PrivateBytes {
    fn drop(&mut self) {
        for b in &mut self.0 {
            unsafe {
                std::ptr::write_volatile(b, 0);
            }
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}
fn unique_match(bytes: &[u8], needle: &[u8]) -> Result<usize> {
    let mut matches = bytes
        .windows(needle.len())
        .enumerate()
        .filter(|(_, s)| *s == needle);
    let first = matches
        .next()
        .map(|(i, _)| i)
        .ok_or("Signature placeholder was not preserved by the PDF writer.")?;
    if matches.next().is_some() {
        return Err("Ambiguous signature placeholder.".into());
    }
    Ok(first)
}
fn reserve(doc: &mut PdfDocument, id: FieldRef, signer: &str) -> Result<()> {
    let mut field = doc.new_indirect(id.field, 0).map_err(err)?;
    if let Some(value) = field.get_dict_inheritable("V").map_err(err)?
        && !value.is_null().map_err(err)?
    {
        return Err("This signature field already has a value.".into());
    }
    if field.get_dict_inheritable("SV").map_err(err)?.is_some()
        || field.get_dict("Lock").map_err(err)?.is_some()
    {
        return Err("Signature seed values and field-lock policies are not supported yet.".into());
    }
    let mut signature=doc.new_object_from_str(&format!("<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached /ByteRange [0 2147483647 2147483647 2147483647] /Contents <{}> >>","00".repeat(RESERVATION))).map_err(err)?;
    signature
        .dict_put("Name", doc.new_string(signer).map_err(err)?)
        .map_err(err)?;
    field
        .dict_put("V", doc.add_object(&signature).map_err(err)?)
        .map_err(err)?;
    Ok(())
}
fn ranges(bytes: &mut [u8]) -> Result<(usize, usize)> {
    let marker = b"[0 2147483647 2147483647 2147483647]";
    let start = unique_match(bytes, marker)?;
    let zero_prefix = format!("<{}", "0".repeat(64));
    let content = unique_match(bytes, zero_prefix.as_bytes())?;
    let end = content
        .checked_add(RESERVATION * 2 + 2)
        .ok_or("Invalid signature reservation")?;
    if end > bytes.len()
        || bytes[end - 1] != b'>'
        || !bytes[content + 1..end - 1].iter().all(|b| *b == b'0')
    {
        return Err("Invalid signature reservation encoding.".into());
    }
    let range = format!("[0 {content} {end} {}]", bytes.len() - end);
    if range.len() > marker.len() {
        return Err("Signature byte ranges exceed the reservation.".into());
    }
    bytes[start..start + marker.len()].fill(b' ');
    bytes[start..start + range.len()].copy_from_slice(range.as_bytes());
    Ok((content, end))
}
pub(crate) fn sign(
    source: &Path,
    document_password: Option<&str>,
    stamp: &SourceStamp,
    request: &SigningRequest,
    current: impl Fn() -> bool,
) -> Result<(PathBuf, SignatureIntegrity)> {
    if !current() {
        return Err("Signing cancelled.".into());
    }
    if request.target.exists() || fs::symlink_metadata(&request.target).is_ok() {
        return Err(
            "Choose a new output filename; signed output never replaces another file.".into(),
        );
    }
    if SourceStamp::read(source)? != *stamp {
        return Err("The PDF changed outside PDFree. Reopen and review it before signing.".into());
    }
    let original = read_bounded(source, MAX_PDF)?;
    if SourceStamp::read(source)? != *stamp {
        return Err("The PDF changed while being read. Reopen it before signing.".into());
    }
    if platform::digest(&original)? != stamp.digest {
        return Err("The PDF content changed after review. Reopen it before signing.".into());
    }
    // Sign the exact reviewed bytes, not a second potentially changed disk open.
    let mut doc = PdfDocument::from_bytes(&original).map_err(err)?;
    if doc.needs_password().map_err(err)?
        && !doc
            .authenticate(document_password.unwrap_or(""))
            .map_err(err)?
    {
        return Err("Could not authenticate the reviewed PDF.".into());
    }
    crate::forms::ensure_writable(&doc)?;
    if !crate::forms::permissions(&doc).contains(mupdf::pdf::Permission::FORM) {
        return Err("This PDF does not permit signing form fields.".into());
    }
    let field = crate::forms::locate(&doc, request.field)?;
    if !field.signable {
        return Err("Choose an unsigned, writable Digital Signature field.".into());
    }
    if field.reason.contains("Scripted") {
        return Err("Scripted signature fields are not supported.".into());
    }
    if !doc.can_be_saved_incrementally() {
        return Err(
            "This PDF requires repair before signing. Save a reviewed copy and reopen it first."
                .into(),
        );
    }
    let certificate = PrivateBytes(read_bounded(&request.certificate, 4 * 1024 * 1024)?);
    let signer = platform::Signer::new(&certificate.0, &request.password)?;
    if !current() {
        return Err("Signing cancelled.".into());
    }
    reserve(&mut doc, request.field, signer.name())?;
    let parent = request
        .target
        .parent()
        .ok_or("Choose a signed output folder")?;
    let stage = loop {
        let path = parent.join(format!(
            ".pdfree-sign-{}-{}.tmp",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_err(err)?
                .as_nanos()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let stage = Staged(path);
                file.write_all(&original).map_err(err)?;
                file.sync_all().map_err(err)?;
                break stage;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(err(e)),
        }
    };
    let mut options = crate::files::options();
    options.set_incremental(true);
    doc.save_with_options(
        stage.0.to_str().ok_or("Output path is not Unicode")?,
        options,
    )
    .map_err(err)?;
    drop(doc);
    let mut bytes = read_bounded(&stage.0, MAX_PDF + RESERVATION * 4)?;
    if !bytes.starts_with(&original) {
        return Err(
            "PDF writer did not preserve the original revision; signing was cancelled.".into(),
        );
    }
    let (begin, end) = ranges(&mut bytes)?;
    let cms = signer.sign(&[&bytes[..begin], &bytes[end..]])?;
    if cms.len() > RESERVATION {
        return Err("The certificate chain exceeds the signature reservation.".into());
    }
    let hex = b"0123456789ABCDEF";
    for (i, b) in cms.iter().enumerate() {
        bytes[begin + 1 + i * 2] = hex[(b >> 4) as usize];
        bytes[begin + 2 + i * 2] = hex[(b & 15) as usize];
    }
    {
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&stage.0)
            .map_err(err)?;
        file.write_all(&bytes).map_err(err)?;
        file.sync_all().map_err(err)?;
    }
    let integrity = verify_signature(&stage.0, request.field.field, document_password)?;
    if !integrity.integrity_verified {
        return Err("The signed output failed cryptographic verification.".into());
    }
    if !current() {
        return Err("Signing cancelled; no output was published.".into());
    }
    if SourceStamp::read(source)? != *stamp {
        return Err("The source changed during signing; no output was published.".into());
    }
    crate::files::move_no_replace(&stage.0, &request.target).map_err(err)?;
    Ok((request.target.clone(), integrity))
}
fn der_length(bytes: &[u8]) -> Result<usize> {
    if bytes.len() < 2 || bytes[0] != 0x30 {
        return Err("Invalid CMS signature.".into());
    }
    let first = bytes[1];
    let (header, length) = if first < 128 {
        (2, first as usize)
    } else {
        let count = (first & 127) as usize;
        if count == 0 || count > 4 || bytes.len() < 2 + count {
            return Err("Invalid CMS length.".into());
        }
        let mut n = 0usize;
        for b in &bytes[2..2 + count] {
            n = n
                .checked_mul(256)
                .and_then(|n| n.checked_add(*b as usize))
                .ok_or("CMS length overflow")?;
        }
        (2 + count, n)
    };
    let end = header + length;
    if end > bytes.len() {
        return Err("Truncated CMS signature.".into());
    }
    Ok(end)
}
pub fn verify_signature(
    path: &Path,
    field: i32,
    password: Option<&str>,
) -> Result<SignatureIntegrity> {
    let bytes = read_bounded(path, MAX_PDF + RESERVATION * 4)?;
    let doc = crate::files::open(path, password)?;
    let signature = doc
        .new_indirect(field, 0)
        .map_err(err)?
        .get_dict_inheritable("V")
        .map_err(err)?
        .ok_or("No signature value")?;
    let range = signature
        .get_dict("ByteRange")
        .map_err(err)?
        .ok_or("No signature byte ranges")?;
    if range.len().map_err(err)? != 4 {
        return Err("Only one contiguous detached PDF signature is supported.".into());
    }
    let mut r = [0usize; 4];
    for (i, n) in r.iter_mut().enumerate() {
        let value = range
            .get_array(i as i32)
            .map_err(err)?
            .ok_or("Invalid byte range")?
            .as_int()
            .map_err(err)?;
        *n = usize::try_from(value).map_err(|_| "Negative byte range")?;
    }
    if r[0] != 0
        || r[1] >= r[2]
        || r[2].checked_add(r[3]) != Some(bytes.len())
        || r[1] >= bytes.len()
        || bytes[r[1]] != b'<'
        || bytes[r[2] - 1] != b'>'
    {
        return Err(
            "Invalid signature coverage; the signature must cover the complete output.".into(),
        );
    }
    let cms = signature
        .get_dict("Contents")
        .map_err(err)?
        .ok_or("No signature contents")?
        .as_bytes()
        .map_err(err)?;
    // Bind the excluded range to this exact /Contents value. Otherwise a
    // crafted PDF could exclude unrelated mutable bytes from integrity checks.
    let raw = &bytes[r[1] + 1..r[2] - 1];
    if raw.len() != cms.len() * 2
        || !raw.as_chunks::<2>().0.iter().zip(&cms).all(|(hex, value)| {
            let digit = |b: u8| (b as char).to_digit(16);
            digit(hex[0])
                .zip(digit(hex[1]))
                .is_some_and(|(a, b)| ((a << 4) | b) as u8 == *value)
        })
    {
        return Err("Signature contents do not match the excluded byte range.".into());
    }
    let end = der_length(&cms)?;
    if cms[end..].iter().any(|b| *b != 0) {
        return Err("Unexpected data after the CMS signature.".into());
    }
    let signer = platform::verify(&cms[..end], &[&bytes[..r[1]], &bytes[r[2]..]])?;
    Ok(SignatureIntegrity {
        signer,
        integrity_verified: true,
        certificate_trust_evaluated: false,
    })
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::ptr;
    use windows_sys::Win32::Security::Cryptography::*;
    pub(super) fn digest(bytes: &[u8]) -> Result<[u8; 32]> {
        let algorithm = [83_u16, 72, 65, 50, 53, 54, 0]; // SHA256
        let mut hash = [0_u8; 32];
        let mut length = 32;
        if unsafe {
            CryptHashCertificate2(
                algorithm.as_ptr(),
                0,
                ptr::null(),
                bytes.as_ptr(),
                bytes.len() as u32,
                hash.as_mut_ptr(),
                &mut length,
            )
        } == 0
            || length != 32
        {
            return Err("Could not hash the reviewed PDF.".into());
        }
        Ok(hash)
    }
    struct Store(HCERTSTORE);
    impl Drop for Store {
        fn drop(&mut self) {
            unsafe {
                CertCloseStore(self.0, 0);
            }
        }
    }
    struct Certificate(*mut CERT_CONTEXT);
    impl Drop for Certificate {
        fn drop(&mut self) {
            unsafe {
                CertFreeCertificateContext(self.0);
            }
        }
    }
    pub(super) struct Signer {
        certs: Vec<Certificate>,
        selected: usize,
        name: String,
        _store: Store,
    }
    fn display(cert: *const CERT_CONTEXT) -> String {
        unsafe {
            let len = CertGetNameStringW(
                cert,
                CERT_NAME_SIMPLE_DISPLAY_TYPE,
                0,
                ptr::null(),
                ptr::null_mut(),
                0,
            );
            let mut text = vec![0u16; len.min(4096) as usize];
            if text.is_empty() {
                return "Certificate signer".into();
            }
            CertGetNameStringW(
                cert,
                CERT_NAME_SIMPLE_DISPLAY_TYPE,
                0,
                ptr::null(),
                text.as_mut_ptr(),
                text.len() as u32,
            );
            String::from_utf16_lossy(
                &text[..text.iter().position(|n| *n == 0).unwrap_or(text.len())],
            )
        }
    }
    impl Signer {
        pub fn new(bytes: &[u8], password: &SecretPassword) -> Result<Self> {
            let blob = CRYPT_INTEGER_BLOB {
                cbData: bytes.len() as u32,
                pbData: bytes.as_ptr().cast_mut(),
            };
            // NO_PERSIST is mandatory: omitting it writes private keys to disk.
            let store = unsafe {
                PFXImportCertStore(
                    &blob,
                    password.0.as_ptr(),
                    PKCS12_NO_PERSIST_KEY | PKCS12_ALWAYS_CNG_KSP,
                )
            };
            if store.is_null() {
                return Err("The certificate file or password could not be opened.".into());
            }
            let store = Store(store);
            let mut certs = vec![];
            let mut selected = None;
            let mut previous = ptr::null_mut();
            loop {
                let cert = unsafe { CertEnumCertificatesInStore(store.0, previous) };
                previous = cert;
                if cert.is_null() {
                    break;
                }
                if certs.len() >= 32 {
                    unsafe {
                        CertFreeCertificateContext(cert);
                    }
                    return Err("Certificate file contains too many certificates.".into());
                }
                let mut key = 0;
                let mut spec = 0;
                let mut free = 0;
                let has_key = unsafe {
                    CryptAcquireCertificatePrivateKey(
                        cert,
                        CRYPT_ACQUIRE_ALLOW_NCRYPT_KEY_FLAG
                            | CRYPT_ACQUIRE_SILENT_FLAG
                            | CRYPT_ACQUIRE_CACHE_FLAG
                            | CRYPT_ACQUIRE_NO_HEALING,
                        ptr::null(),
                        &mut key,
                        &mut spec,
                        &mut free,
                    )
                } != 0;
                if has_key {
                    if free != 0 {
                        unsafe {
                            if spec == CERT_NCRYPT_KEY_SPEC {
                                NCryptFreeObject(key);
                            } else {
                                CryptReleaseContext(key, 0);
                            }
                        }
                    }
                    if selected.is_some() {
                        unsafe {
                            CertFreeCertificateContext(cert);
                        }
                        return Err(
                            "Choose a PKCS#12 file containing exactly one signing key.".into()
                        );
                    }
                    if unsafe { CertVerifyTimeValidity(ptr::null(), (*cert).pCertInfo) } != 0 {
                        unsafe {
                            CertFreeCertificateContext(cert);
                        }
                        return Err("The signing certificate is expired or not yet valid.".into());
                    }
                    selected = Some(certs.len());
                }
                let duplicate = unsafe { CertDuplicateCertificateContext(cert) };
                if duplicate.is_null() {
                    unsafe {
                        CertFreeCertificateContext(cert);
                    }
                    return Err("Could not retain the signing certificate.".into());
                }
                certs.push(Certificate(duplicate));
            }
            let selected =
                selected.ok_or("No usable signing key was found in the certificate file.")?;
            let name = display(certs[selected].0);
            Ok(Self {
                certs,
                selected,
                name,
                _store: store,
            })
        }
        pub fn name(&self) -> &str {
            &self.name
        }
        pub fn sign(&self, data: &[&[u8]]) -> Result<Vec<u8>> {
            let mut certs = self.certs.iter().map(|c| c.0).collect::<Vec<_>>();
            let mut params = CRYPT_SIGN_MESSAGE_PARA {
                cbSize: std::mem::size_of::<CRYPT_SIGN_MESSAGE_PARA>() as u32,
                dwMsgEncodingType: PKCS_7_ASN_ENCODING | X509_ASN_ENCODING,
                pSigningCert: self.certs[self.selected].0,
                cMsgCert: certs.len() as u32,
                rgpMsgCert: certs.as_mut_ptr(),
                dwFlags: CRYPT_MESSAGE_SILENT_KEYSET_FLAG,
                ..Default::default()
            };
            params.HashAlgorithm.pszObjId = szOID_NIST_sha256.cast_mut();
            let pointers = data.iter().map(|d| d.as_ptr()).collect::<Vec<_>>();
            let lengths = data.iter().map(|d| d.len() as u32).collect::<Vec<_>>();
            let mut length = 0;
            if unsafe {
                CryptSignMessage(
                    &params,
                    1,
                    data.len() as u32,
                    pointers.as_ptr(),
                    lengths.as_ptr(),
                    ptr::null_mut(),
                    &mut length,
                )
            } == 0
            {
                return Err(format!(
                    "Could not prepare the digital signature: {}",
                    std::io::Error::last_os_error()
                ));
            }
            if length as usize > RESERVATION {
                return Err("The signing certificate chain is too large.".into());
            }
            let mut cms = vec![0; length as usize];
            if unsafe {
                CryptSignMessage(
                    &params,
                    1,
                    data.len() as u32,
                    pointers.as_ptr(),
                    lengths.as_ptr(),
                    cms.as_mut_ptr(),
                    &mut length,
                )
            } == 0
            {
                return Err(format!(
                    "Certificate signing failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            cms.truncate(length as usize);
            Ok(cms)
        }
    }
    pub(super) fn verify(cms: &[u8], data: &[&[u8]]) -> Result<String> {
        let params = CRYPT_VERIFY_MESSAGE_PARA {
            cbSize: std::mem::size_of::<CRYPT_VERIFY_MESSAGE_PARA>() as u32,
            dwMsgAndCertEncodingType: PKCS_7_ASN_ENCODING | X509_ASN_ENCODING,
            ..Default::default()
        };
        let pointers = data.iter().map(|d| d.as_ptr()).collect::<Vec<_>>();
        let lengths = data.iter().map(|d| d.len() as u32).collect::<Vec<_>>();
        let mut cert = ptr::null_mut();
        let valid = unsafe {
            CryptVerifyDetachedMessageSignature(
                &params,
                0,
                cms.as_ptr(),
                cms.len() as u32,
                data.len() as u32,
                pointers.as_ptr(),
                lengths.as_ptr(),
                &mut cert,
            )
        } != 0;
        let signer = if cert.is_null() {
            String::new()
        } else {
            let certificate = Certificate(cert);
            display(certificate.0)
        };
        if !valid {
            return Err("Digital signature integrity verification failed; the output may have been modified.".into());
        }
        Ok(signer)
    }
}
#[cfg(not(windows))]
mod platform {
    use super::*;
    pub(super) fn digest(_bytes: &[u8]) -> Result<[u8; 32]> {
        Err("Certificate signing requires Windows.".into())
    }
    pub(super) struct Signer;
    impl Signer {
        pub fn new(_bytes: &[u8], _password: &SecretPassword) -> Result<Self> {
            Err("Certificate signing requires Windows.".into())
        }
        pub fn name(&self) -> &str {
            ""
        }
        pub fn sign(&self, _data: &[&[u8]]) -> Result<Vec<u8>> {
            Err("Certificate signing requires Windows.".into())
        }
    }
    pub(super) fn verify(_cms: &[u8], _data: &[&[u8]]) -> Result<String> {
        Err("Signature verification requires Windows.".into())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn source_stamp_detects_same_length_replacement_with_restored_mtime() {
        let path = std::env::temp_dir().join(format!("pdfree-stamp-{}.tmp", std::process::id()));
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        f.write_all(b"reviewed source").unwrap();
        drop(f);
        let original = SourceStamp::read(&path).unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        fs::write(&path, b"replaced source").unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        let changed = SourceStamp::read(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert_ne!(original, changed);
    }
}
