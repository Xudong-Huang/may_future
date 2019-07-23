# may_future

[![Travis Build Status](https://travis-ci.org/Xudong-Huang/may_future.svg?branch=master)](https://travis-ci.org/Xudong-Huang/may_future)


Future runtime in [may][may]


## Overview

**may_future** is a runtime library which allows to execute futures on the coroutine context. Specifically the `block_on` API of the runtime would not block the underlying worker thread that is scheduling the coroutine. It also supply a global static runtime named `may_future::RT` which is convenient to spawn or block_on futures at hand.

Internally it use the tokio runtime for implementation. This opens the possibility  to integrate any features from the tokio stack.

## Example

```rust
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
```



# License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.

<!--refs-->
[may]:https://github.com/Xudong-Huang/may
