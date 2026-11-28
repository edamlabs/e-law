use std::fs;

use law_model::Report;
use law_report::Tally;

use crate::cli::ReportArgs;
use crate::exit::Exit;

impl ReportArgs {
    fn converted(&self) -> Result<Exit, Exit> {
        let path = self.path.display();
        let text = fs::read_to_string(&self.path).map_err(|error| Exit::faults([format!("{path}: {error}")]))?;
        let report: Report =
            serde_json::from_str(&text).map_err(|error| Exit::faults([format!("{path}: not a report: {error}")]))?;
        Exit::emit(|out| law_report::write(&report, Tally::default(), self.format, out))?;
        Ok(Exit::Clean)
    }

    /// A saved report, read and validated, written in another format; no project is needed.
    #[must_use]
    pub fn run(&self) -> Exit {
        self.converted().unwrap_or_else(|exit| exit)
    }
}
