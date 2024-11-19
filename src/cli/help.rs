// use crate::app::WelcomeMessage;
use crate::cli::ProcessCommand;
use crate::context::Context;
use crate::Result;
use async_trait::async_trait;
use std::fmt::Debug;

#[derive(Debug)]
pub struct Clear;

#[async_trait]
impl ProcessCommand for Clear {
    async fn process_command(&self, _context: &Context) -> Result<()> {
        if cfg!(target_os = "windows") {
            std::process::Command::new("cmd")
                .args(&["/C", "cls"])
                .status()
                .unwrap();
        } else {
            std::process::Command::new("clear").status().unwrap();
        }
        // WelcomeMessage::midas_header();
        // WelcomeMessage::midas_help();

        Ok(())
    }
}
