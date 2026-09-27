//! Capture normalized frontend files and translate spans into an owned arena.

use crate::js;
use crate::program::{Source, Sources};
use rustc_middle::ty::TyCtxt;
use rustc_span::Span;

pub(super) struct CapturedSources {
    offsets: Vec<(u32, u32, u32)>,
    pub output: Sources,
}

impl CapturedSources {
    pub fn new(tcx: TyCtxt<'_>) -> Self {
        let mut output = Sources {
            text: String::new(),
            files: Vec::new(),
        };
        let mut offsets = Vec::new();
        let mut line = 0;
        for file in tcx.sess.source_map().files().iter() {
            let Some(text) = file.src.as_deref() else { continue };
            let start = output.text.len() as u32;
            offsets.push((file.start_pos.0, file.end_position().0, start));
            output.files.push(Source {
                path: file.name.clone().into_local_path(),
                text: text.to_string(),
                line,
            });
            output.text.push_str(text);
            output.text.push('\n');
            line += text.bytes().filter(|&byte| byte == b'\n').count() as u32 + 1;
        }
        Self { offsets, output }
    }

    pub fn span(&self, span: Span) -> js::Span {
        let span = span.source_callsite();
        let index = self.offsets.partition_point(|&(start, _, _)| start <= span.lo().0);
        let Some(&(start, end, offset)) = index.checked_sub(1).map(|i| &self.offsets[i]) else {
            return js::Span::NONE;
        };
        if span.hi().0 > end {
            return js::Span::NONE;
        }
        js::Span {
            lo: offset + span.lo().0 - start,
            hi: offset + span.hi().0 - start,
        }
    }
}
