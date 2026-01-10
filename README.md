# cargo-tree-tui

## installation
```bash
$ cargo install --git https://github.com/orhun/cargo-tree-tui.git
```

## usage
in the root of your cargo project:
```bash
$ cargo tree-tui
```

## benchmarks

Run all benchmarks:
```bash
cargo bench
```

Run specific benchmark categories:
```bash
cargo bench -- cli          # CLI argument parsing
cargo bench -- render       # Frame rendering
cargo bench -- load         # Dependency tree loading
cargo bench -- terminal     # Terminal initialization (ignored by default)
```

The benchmarks measure:
- CLI parsing performance with various argument combinations
- Frame rendering times for different terminal sizes
- Dependency tree loading performance
- Terminal initialization (requires BENCH_TERMINAL=true)
