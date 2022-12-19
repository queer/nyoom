use std::path::Path;
use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn walk(path: &str) {
    nyoom::walk(Path::new(path), |_path, is_dir| is_dir).unwrap();
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("walk /usr with lib: ");
    group.warm_up_time(Duration::from_secs(180));
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("nyoom", |b| b.iter(|| walk(black_box("/usr"))));
    group.bench_function("ignore", |b| {
        b.iter(|| {
            ignore::WalkBuilder::new(black_box("/usr"))
                .build_parallel()
                .run(|| Box::new(|_| ignore::WalkState::Continue))
        })
    });
    group.bench_function("walkdir", |b| {
        b.iter(|| {
            walkdir::WalkDir::new(black_box("/usr"))
                .into_iter()
                .filter_map(|e| e.ok())
                .collect::<Vec<_>>()
        })
    });
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
