
// A JS iterator lent as a `&mut` (ADR 0071): it steps `it`, and what takes
// from it and stops early, as `take` or a loop that breaks does, closes
// this, not `it`, which its lender goes on stepping.
function $lent(it) {
  return Iterator.from({ next: () => it.next() });
}
