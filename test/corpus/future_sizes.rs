// `size_of_val` of an `async fn`'s future, and of one of a generic `async fn`
// taking another: rustc's size for it, which 1.98 finds too generic to lay
// out but in a fully monomorphic environment, as codegen asks (ADR 0109).
// Sizes differ between targets, so only that there is one is printed.
async fn test(_arg: [u8; 16]) {}

async fn use_future(fut: impl std::future::Future<Output = ()>) {
    fut.await
}

fn main() {
    let simple = std::mem::size_of_val(&test([0; 16]));
    let nested = std::mem::size_of_val(&use_future(use_future(test([0; 16]))));
    println!("{} {}", simple >= 16, nested > simple);
}
