// A channel on one thread is a queue (ADR 0142): `send` puts an item in it,
// `recv` takes the first out. Its ends have destructors: once every sender
// is dropped, `recv` of an empty one is an error, and once the receiver is,
// `send` is.
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

struct Job {
    id: u32,
    name: String,
}

fn produce(tx: Sender<Job>) {
    for id in 1..=3 {
        tx.send(Job { id, name: format!("job {id}") }).unwrap();
    }
}

fn drain(rx: &Receiver<Job>) -> Vec<String> {
    let mut names = Vec::new();
    while let Ok(job) = rx.recv() {
        names.push(format!("{} {}", job.id, job.name));
    }
    names
}

fn main() {
    let (tx, rx) = mpsc::channel();
    let tx2 = tx.clone();
    produce(tx);
    tx2.send(Job { id: 9, name: "late".to_string() }).unwrap();
    if let Ok(job) = rx.try_recv() {
        println!("first {}", job.id);
    }
    drop(tx2);
    println!("{:?}", drain(&rx));
    println!("{}", matches!(rx.try_recv(), Err(TryRecvError::Disconnected)));

    let (tx, rx) = mpsc::channel::<u8>();
    drop(rx);
    println!("{}", tx.send(5).is_err());

    let (tx, rx) = mpsc::channel();
    {
        let notify = move || tx.send("done").unwrap();
        notify();
        println!("{:?}", rx.recv());
        println!("{}", matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }
    println!("{}", rx.recv().is_err());
}
