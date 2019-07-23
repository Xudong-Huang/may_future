#![feature(async_await)]

use may::{coroutine, go};
use may_future::RT;

fn main() {
    RT.block_on(async {
        println!("hello world");
    });

    coroutine::scope(|s| {
        for i in 0..100 {
            go!(s, move || {
                RT.block_on(async {
                    println!("hello world from coroutine {}", i);
                });
            });
        }
    });
}
