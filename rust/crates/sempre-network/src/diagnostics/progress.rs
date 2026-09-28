use super::{DiagnosticFinding, DiagnosticLayer};

#[derive(Clone, Debug)]
pub enum DiagnosticProgress {
    Started { layer: &'static str },
    Completed { layer: DiagnosticLayer },
}

pub(super) struct DiagnosticLog<F> {
    pub layers: Vec<DiagnosticLayer>,
    pub findings: Vec<DiagnosticFinding>,
    on_progress: F,
}

impl<F> DiagnosticLog<F>
where
    F: FnMut(DiagnosticProgress),
{
    pub fn new(on_progress: F) -> Self {
        Self {
            layers: Vec::new(),
            findings: Vec::new(),
            on_progress,
        }
    }

    pub fn start(&mut self, layer: &'static str) {
        (self.on_progress)(DiagnosticProgress::Started { layer });
    }

    pub fn complete(&mut self, layer: DiagnosticLayer) {
        (self.on_progress)(DiagnosticProgress::Completed {
            layer: layer.clone(),
        });
        self.layers.push(layer);
    }

    pub fn complete_all(&mut self, layers: Vec<DiagnosticLayer>) {
        for layer in layers {
            self.complete(layer);
        }
    }
}
