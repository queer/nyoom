# nyoom

A sorta-fast cross-platform filesystem walker

MSRV 1.66

[crates.io](https://crates.io/crates/nyoom)

## Example

```rust
let walk_results = nyoom::walk("path/to/dir")?;
println!("visited {} paths", walk_results.paths.len());
```

## Benchmarks

`cargo bench` and see for yourself.

![](https://cdn.mewna.xyz/2022/12/20/53NfwQNdAC7IL.png)

Benchmarks are run with the default settings for each library, with the
exception that libraries are specifically configured to use as many threads as
there are CPU cores, if possible, if that is not the default setting.

Numbers subject to change over time, may not be up-to-date, may be different in
your test environment, etc. Do your own testing to figure out if this is a
workable solution for you.
