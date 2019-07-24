#![feature(async_await)]

use may::{coroutine, go};
use may_future::RT;

use std::sync::atomic::{AtomicUsize, Ordering};

fn main() {
    RT.block_on(async {
        println!("hello world");
    });

    let j = AtomicUsize::new(0);
    coroutine::scope(|s| {
        for i in 0..100 {
            let j = &j;
            go!(s, move || {
                RT.block_on_local(async {
                    println!(
                        "hello world from coroutine {}, data={}",
                        i,
                        j.fetch_add(i, Ordering::Relaxed) + i
                    );
                });
            });
        }
    });
}
