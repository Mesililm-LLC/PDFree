#![cfg(windows)]
use pdf_engine::{EditCommand, Engine, Event, SecretPassword, SigningRequest, verify_signature};
use std::{path::PathBuf, process::Command, time::Duration};
use viewer_core::{FieldKind, FieldSpec, FormEdit, TextRect};
fn wait(e: &Engine, done: impl Fn(&Event) -> bool) -> Event {
    loop {
        let event = e.events.recv_timeout(Duration::from_secs(30)).unwrap();
        if let Event::FileError { message, .. } = &event {
            panic!("{message}");
        }
        if done(&event) {
            return event;
        }
    }
}
fn command(args: &[&str]) {
    let output = Command::new(
        std::env::var_os("PDFREE_OPENSSL")
            .unwrap_or_else(|| "C:/Program Files/Git/usr/bin/openssl.exe".into()),
    )
    .args(args)
    .output()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn retry_sharing(action: impl FnMut() -> std::io::Result<()>) {
    let mut action = action;
    let mut attempt = 0;
    loop {
        match action() {
            Ok(()) => return,
            Err(e) if e.raw_os_error() == Some(32) && attempt < 10 => {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(10 * attempt));
            }
            Err(e) => panic!("{e}"),
        }
    }
}

#[test]
fn incremental_signature_verifies_independently_and_detects_tampering() {
    run(false);
}
#[test]
fn encrypted_incremental_signature_preserves_password_and_source() {
    run(true);
}
fn run(encrypted: bool) {
    let dir = std::env::temp_dir().join(format!(
        "pdfree-sign-test-{}-{encrypted}",
        std::process::id()
    ));
    std::fs::create_dir(&dir).unwrap();
    let key = dir.join("key.pem");
    let cert = dir.join("cert.pem");
    let pfx = dir.join("test.pfx");
    command(&[
        "req",
        "-x509",
        "-newkey",
        "rsa:2048",
        "-keyout",
        key.to_str().unwrap(),
        "-out",
        cert.to_str().unwrap(),
        "-days",
        "2",
        "-nodes",
        "-subj",
        "/CN=PDFree Test Signer",
    ]);
    command(&[
        "pkcs12",
        "-export",
        "-inkey",
        key.to_str().unwrap(),
        "-in",
        cert.to_str().unwrap(),
        "-out",
        pfx.to_str().unwrap(),
        "-passout",
        "pass:pdfree-test",
        "-keypbe",
        "AES-256-CBC",
        "-certpbe",
        "AES-256-CBC",
        "-macalg",
        "sha256",
    ]);
    let e = Engine::spawn(1);
    e.open(
        1,
        1,
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/blank.pdf"),
    );
    wait(&e, |v| matches!(v, Event::Opened { .. }));
    let spec = FieldSpec {
        page: 0,
        name: "digital_signature".into(),
        label: "Digital signature".into(),
        kind: FieldKind::DigitalSignature,
        rect: TextRect {
            left: 40.,
            top: 40.,
            right: 260.,
            bottom: 80.,
        },
        required: false,
        read_only: false,
        options: vec![],
        group: String::new(),
        review: false,
        date_format: "YYYY-MM-DD".into(),
        decimals: 2,
    };
    assert!(e.edit(1, 0, 1, 2, EditCommand::Form(FormEdit::Create(vec![spec]))));
    let rev = match wait(&e, |v| matches!(v, Event::Changed { .. })) {
        Event::Changed { revision, .. } => revision,
        _ => unreachable!(),
    };
    e.form_pages(1, rev, vec![0]);
    let mut field = match wait(&e, |v| matches!(v, Event::Forms { .. })) {
        Event::Forms { mut fields, .. } => fields.remove(0),
        _ => unreachable!(),
    };
    let mut unsigned = dir.join("unsigned.pdf");
    assert!(e.save_as(1, 2, unsigned.clone(), vec![]));
    wait(&e, |v| matches!(v, Event::FileDone { .. }));
    let password = encrypted.then_some("pdfree-document");
    if encrypted {
        let pdf = mupdf::Document::open(unsigned.to_str().unwrap())
            .and_then(mupdf::pdf::PdfDocument::try_from)
            .unwrap();
        let mut options = mupdf::pdf::PdfWriteOptions::default();
        options
            .set_encryption(mupdf::pdf::Encryption::Aes256)
            .set_owner_password("owner")
            .set_user_password(password.unwrap());
        let target = dir.join("encrypted-unsigned.pdf");
        pdf.save_with_options(target.to_str().unwrap(), options)
            .unwrap();
        drop(pdf);
        unsigned = target;
    }
    e.open_with_password(1, 3, unsigned.clone(), password.map(str::to_string));
    wait(&e, |v| matches!(v, Event::Opened { .. }));
    let rev = 0;
    e.form_pages(1, rev, vec![0]);
    if let Event::Forms { mut fields, .. } = wait(&e, |v| matches!(v, Event::Forms { .. })) {
        field = fields.remove(0);
    }
    let request = |target: PathBuf, secret: &str| SigningRequest {
        field: field.id,
        certificate: pfx.clone(),
        target,
        password: SecretPassword::new(secret.into()),
    };
    let failure = |operation| loop {
        if let Event::FileError {
            operation: op,
            message,
            ..
        } = e.events.recv_timeout(Duration::from_secs(30)).unwrap()
        {
            assert_eq!(op, operation);
            break message;
        }
    };
    let failed = dir.join("failed.pdf");
    assert!(e.sign(1, rev, 10, request(failed.clone(), "wrong")));
    assert!(failure(10).contains("password"));
    assert!(!failed.exists());
    assert!(e.sign(1, rev, 11, request(unsigned.clone(), "pdfree-test")));
    assert!(failure(11).contains("filename"));
    assert!(e.sign(1, rev, 12, request(failed.clone(), "pdfree-test")));
    e.cancel_signing();
    assert!(failure(12).contains("cancelled"));
    assert!(!failed.exists());
    let original = std::fs::read(&unsigned).unwrap();
    let signed = dir.join("signed.pdf");
    assert!(e.sign(
        1,
        rev,
        3,
        SigningRequest {
            field: field.id,
            certificate: pfx.clone(),
            target: signed.clone(),
            password: SecretPassword::new("pdfree-test".into())
        }
    ));
    let integrity = match wait(&e, |v| matches!(v, Event::Signed { .. })) {
        Event::Signed { integrity, .. } => integrity,
        _ => unreachable!(),
    };
    assert!(integrity.integrity_verified);
    assert!(!integrity.certificate_trust_evaluated);
    assert_eq!(std::fs::read(&unsigned).unwrap(), original);
    assert!(std::fs::read(&signed).unwrap().starts_with(&original));
    assert!(
        verify_signature(&signed, field.id.field, password)
            .unwrap()
            .integrity_verified
    );
    // Independent cryptographic implementation, using byte ranges parsed from the PDF.
    let mut pdf = mupdf::Document::open(signed.to_str().unwrap())
        .and_then(mupdf::pdf::PdfDocument::try_from)
        .unwrap();
    if let Some(password) = password {
        assert!(pdf.authenticate(password).unwrap());
    }
    let v = pdf
        .new_indirect(field.id.field, 0)
        .unwrap()
        .get_dict("V")
        .unwrap()
        .unwrap();
    let range = v.get_dict("ByteRange").unwrap().unwrap();
    let start = range.get_array(1).unwrap().unwrap().as_int().unwrap() as usize;
    let end = range.get_array(2).unwrap().unwrap().as_int().unwrap() as usize;
    let cms = v.get_dict("Contents").unwrap().unwrap().as_bytes().unwrap();
    drop(pdf);
    let bytes = std::fs::read(&signed).unwrap();
    let mut content = bytes[..start].to_vec();
    content.extend_from_slice(&bytes[end..]);
    let cms_path = dir.join("signature.der");
    let content_path = dir.join("signed-content.bin");
    std::fs::write(&cms_path, cms).unwrap();
    std::fs::write(&content_path, &content).unwrap();
    command(&[
        "cms",
        "-verify",
        "-binary",
        "-inform",
        "DER",
        "-in",
        cms_path.to_str().unwrap(),
        "-content",
        content_path.to_str().unwrap(),
        "-noverify",
    ]);
    let mut tampered = bytes;
    tampered[7] = if tampered[7] == b'4' { b'7' } else { b'4' };
    let tampered_path = dir.join("tampered.pdf");
    std::fs::write(&tampered_path, tampered).unwrap();
    assert!(verify_signature(&tampered_path, field.id.field, password).is_err());
    if let Some(evidence) = std::env::var_os("PDFREE_FORM_EVIDENCE_DIR") {
        std::fs::create_dir_all(&evidence).unwrap();
        std::fs::copy(
            &signed,
            PathBuf::from(evidence).join(if encrypted {
                "digitally-signed-encrypted.pdf"
            } else {
                "digitally-signed.pdf"
            }),
        )
        .unwrap();
    }
    // A source modified externally after review must never produce an output.
    let mut changed = original.clone();
    changed.extend_from_slice(b"\n% external change\n");
    retry_sharing(|| std::fs::write(&unsigned, &changed));
    assert!(e.sign(1, rev, 13, request(failed.clone(), "pdfree-test")));
    assert!(failure(13).contains("outside"));
    assert!(!failed.exists());
    e.open_with_password(2, 4, signed.clone(), password.map(str::to_string));
    let info = match wait(&e, |v| matches!(v, Event::Opened { document: 2, .. })) {
        Event::Opened { info, .. } => info,
        _ => unreachable!(),
    };
    assert!(info.signed);
    assert!(!info.can_form && !info.can_annotate && !info.can_edit);
    drop(e);
    let resolved = std::fs::canonicalize(&dir).unwrap();
    assert!(resolved.starts_with(std::fs::canonicalize(std::env::temp_dir()).unwrap()));
    assert!(
        resolved
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("pdfree-sign-test-")
    );
    retry_sharing(|| std::fs::remove_dir_all(&resolved));
}
