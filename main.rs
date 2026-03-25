use anyhow::Result;
use rts_student_b_gcs::gcs::network::run_receiver;

#[tokio::main]
async fn main() -> Result<()> {
    println!("[GCS] Starting Ground Control Station receiver...");
    run_receiver().await
}