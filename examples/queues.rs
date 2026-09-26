// `VecDeque` and `BinaryHeap`, a crate's own `Index`, a generic `Display`,
// closures in a struct, and `std::cmp::Reverse` (ADR 0068). A deque is a JS
// array; a heap is one too, kept in the order Rust's own heap keeps it.

use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::fmt;
use std::ops::Index;

pub struct Stack<T> {
    items: Vec<T>,
}
impl<T: Clone + fmt::Debug> Stack<T> {
    pub fn new() -> Self {
        Stack { items: Vec::new() }
    }
    pub fn push(&mut self, x: T) {
        self.items.push(x);
    }
    pub fn pop(&mut self) -> Option<T> {
        self.items.pop()
    }
    pub fn peek(&self) -> Option<&T> {
        self.items.last()
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

pub struct Grid {
    w: usize,
    cells: Vec<u32>,
}
impl Index<(usize, usize)> for Grid {
    type Output = u32;
    fn index(&self, (r, c): (usize, usize)) -> &u32 {
        &self.cells[r * self.w + c]
    }
}

pub struct Labeled<T> {
    pub label: String,
    pub value: T,
}
impl<T: fmt::Display> fmt::Display for Labeled<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.label, self.value)
    }
}

pub struct Pipeline {
    steps: Vec<Box<dyn Fn(i32) -> i32>>,
}
impl Pipeline {
    pub fn run(&self, x: i32) -> i32 {
        self.steps.iter().fold(x, |acc, f| f(acc))
    }
}

fn first_even_square(v: &[i32]) -> Option<i32> {
    let e = v.iter().find(|&&x| x % 2 == 0)?;
    Some(e * e)
}

pub fn dijkstra(edges: &[(usize, usize, u32)], n: usize, src: usize) -> Vec<Option<u32>> {
    let mut adj: Vec<Vec<(usize, u32)>> = vec![Vec::new(); n];
    for &(a, b, w) in edges {
        adj[a].push((b, w));
        adj[b].push((a, w));
    }
    let mut dist: Vec<Option<u32>> = vec![None; n];
    let mut heap = BinaryHeap::new();
    dist[src] = Some(0);
    heap.push(Reverse((0u32, src)));
    while let Some(Reverse((d, u))) = heap.pop() {
        if dist[u].is_some_and(|best| d > best) {
            continue;
        }
        for &(v, w) in &adj[u] {
            let nd = d + w;
            if dist[v].is_none_or(|best| nd < best) {
                dist[v] = Some(nd);
                heap.push(Reverse((nd, v)));
            }
        }
    }
    dist
}

pub fn bfs_order(n: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut seen = vec![false; n];
    let mut q = VecDeque::new();
    let mut order = vec![];
    q.push_back(0);
    seen[0] = true;
    while let Some(u) = q.pop_front() {
        order.push(u);
        for &(a, b) in edges {
            if a == u && !seen[b] {
                seen[b] = true;
                q.push_back(b);
            }
        }
    }
    order
}

// Ordered by priority alone: which of two equal ones comes out first shows
// whether the heap moves its items as Rust's does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub pri: u32,
    pub name: char,
}
impl Ord for Task {
    fn cmp(&self, other: &Self) -> Ordering {
        self.pri.cmp(&other.pri)
    }
}
impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn heaps() -> String {
    let mut out = String::new();
    let mut h = BinaryHeap::new();
    for (i, pri) in [3u32, 1, 3, 5, 1, 3, 2, 5, 0].iter().enumerate() {
        h.push(Task {
            pri: *pri,
            name: (b'a' + i as u8) as char,
        });
        out += &format!("{:?}\n", h.iter().map(|t| t.name).collect::<String>());
    }
    out += &format!("peek {:?} len {}\n", h.peek().map(|t| t.name), h.len());
    let mut popped = String::new();
    for _ in 0..4 {
        popped.push(h.pop().unwrap().name);
        popped += &h.iter().map(|t| t.name).collect::<String>();
        popped.push(' ');
    }
    out += &format!("{popped}\n");
    let sorted: String = h.clone().into_sorted_vec().iter().map(|t| t.name).collect();
    let from = BinaryHeap::from(vec![4, 8, 1, 9, 9, 2, 7, 3]);
    let collected: BinaryHeap<i32> = [5, 1, 5, 2, 8].into_iter().collect();
    out += &format!(
        "{sorted} {:?} {:?} {:?}\n",
        from,
        collected.into_vec(),
        from.clone().into_sorted_vec()
    );
    let mut empty: BinaryHeap<u32> = BinaryHeap::new();
    out += &format!("{:?} {:?} {}\n", empty.pop(), empty.peek(), empty.is_empty());
    out
}

pub fn deques() -> String {
    let mut d: VecDeque<i32> = VecDeque::from(vec![3, 4]);
    d.push_front(2);
    d.push_back(5);
    d.push_front(1);
    let (front, back) = (d.front().copied(), d.back().copied());
    let removed = (d.remove(1), d.remove(10));
    let popped = (d.pop_front(), d.pop_back());
    d.retain(|&x| x != 4);
    format!(
        "{d:?} {front:?} {back:?} {removed:?} {popped:?} {} {}",
        d.len(),
        d.contains(&3)
    )
}

pub fn reverses(a: u32, b: u32) -> (bool, bool, u32, String, Vec<u32>) {
    let x = Reverse((a, 1));
    let y = Reverse((b, 2));
    let Reverse((p, _)) = x;
    let mut v = vec![a, b, 7, 1];
    v.sort_by_key(|&k| Reverse(k));
    (x < y, x.max(y) == x, p, format!("{:?}", y), v)
}

pub fn report() -> String {
    let mut out = String::new();
    let mut s: Stack<&str> = Stack::new();
    for w in ["a", "b", "c"] {
        s.push(w);
    }
    out += &format!("{:?} {:?} {}\n", s.pop(), s.peek(), s.len());
    let g = Grid {
        w: 3,
        cells: (0..9).collect(),
    };
    out += &format!("{} {}\n", g[(1, 2)], g[(2, 0)]);
    let l = Labeled {
        label: "pi".into(),
        value: 3.25,
    };
    out += &format!(
        "{l} {}\n",
        Labeled {
            label: "n".into(),
            value: 7
        }
    );
    let p = Pipeline {
        steps: vec![Box::new(|x| x + 1), Box::new(|x| x * 10), Box::new(move |x| x - 3)],
    };
    out += &format!(
        "{} {:?} {:?}\n",
        p.run(4),
        first_even_square(&[3, 5, 6, 8]),
        first_even_square(&[1])
    );
    out += &format!("{:?}\n", dijkstra(&[(0, 1, 4), (0, 2, 1), (2, 1, 2), (1, 3, 5)], 5, 0));
    out += &format!("{:?}\n", bfs_order(5, &[(0, 1), (0, 2), (1, 3), (2, 4)]));
    let mut words: HashMap<usize, Vec<&str>> = HashMap::new();
    for w in "the quick brown fox jumps over the lazy dog".split(' ') {
        words.entry(w.chars().count()).or_default().push(w);
    }
    let mut lens: Vec<_> = words.keys().copied().collect();
    lens.sort_unstable_by(|a, b| b.cmp(a));
    out += &format!("{lens:?} {:?}\n", words[&5]);
    out += &heaps();
    out += &format!("{}\n{:?} {:?}\n", deques(), reverses(1, 2), reverses(5, 3));
    out
}
