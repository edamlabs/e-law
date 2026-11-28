use law_engine::TrialRunner;

use crate::cli::TestArgs;
use crate::exit::Exit;

impl TestArgs {
    fn tested(&self) -> Result<Exit, Exit> {
        let opened = self.project.open(&self.catalogues)?;
        let cases = opened.cases()?;
        let runner =
            TrialRunner::new(&opened.loaded, &opened.selection, cases).map_err(|faults| opened.faults(faults))?;
        let proof = runner.run();
        let format = self.output_format;
        Exit::emit(|out| law_report::trials(&proof.trials, format, out))?;
        match (proof.faults.is_empty(), proof.trials.failed) {
            (false, _) => Err(opened.faults(proof.faults)),
            (true, 0) => Ok(Exit::Clean),
            (true, _) => Ok(Exit::Findings),
        }
    }

    /// The test, composed: open, plan every selected rule alone, prove each against its cases, write; faults to stderr.
    #[must_use]
    pub fn run(&self) -> Exit {
        self.tested().unwrap_or_else(|exit| exit)
    }
}
