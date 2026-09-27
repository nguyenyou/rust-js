// A frame message is untyped input, even when its sender is the current frame.
export function readReport(event) {
  const report = event.data;
  const count = value => Number.isInteger(value) && value >= 0 && value <= 0xffffffff;
  if (!report || typeof report !== "object" || !count(report.run)) return undefined;
  const kinds = [report.error !== undefined, report.ran !== undefined, report.tested !== undefined];
  if (kinds.filter(Boolean).length !== 1) return undefined;
  if (typeof report.error === "string") return { run: report.run, error: report.error };
  if (report.ran === true) return { run: report.run, ran: true };
  const tested = report.tested;
  if (tested && count(tested.passed) && count(tested.failed) && count(tested.ignored)) {
    return { run: report.run, tested: { passed: tested.passed, failed: tested.failed, ignored: tested.ignored } };
  }
  return undefined;
}
