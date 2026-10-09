use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Clone)]
pub struct Trace {
    start: Instant,
    output: Option<Arc<Mutex<BufWriter<File>>>>,
}

impl Trace {
    pub fn new(start: Instant, path: Option<PathBuf>) -> std::io::Result<Self> {
        Ok(Self {
            start,
            output: path
                .map(File::create)
                .transpose()?
                .map(|f| Arc::new(Mutex::new(BufWriter::new(f)))),
        })
    }
    pub fn event(&self, name: &str, values: serde_json::Value) {
        let Some(output) = &self.output else {
            return;
        };
        let value = serde_json::json!({"event":name,"elapsed_ms":self.start.elapsed().as_secs_f64()*1000.0,"data":values});
        if let Ok(mut file) = output.lock() {
            let _ = writeln!(file, "{value}");
            let _ = file.flush();
        }
    }
}
