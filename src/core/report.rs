use crate::core::metadata::TrackMetadata;

pub struct DownloadReport {
    pub succeeded: Vec<String>,
    pub failed: Vec<String>,
}

impl DownloadReport {
    pub fn print(&self) {
        println!("\n{}", "=".repeat(88));
        println!("DOWNLOAD REPORT");
        println!("{}", "=".repeat(88));
        println!("Succeeded: {}/{}", self.succeeded.len(), self.succeeded.len() + self.failed.len());

        if !self.succeeded.is_empty() {
            println!("\n  Downloaded:");
            for s in &self.succeeded {
                println!("    [OK] {s}");
            }
        }

        if !self.failed.is_empty() {
            println!("\n  Failed:");
            for f in &self.failed {
                println!("    [FAIL] {f}");
            }
        }
        println!();
    }
}
