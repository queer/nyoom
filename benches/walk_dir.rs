use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("walk /usr with lib: ");
    group.warm_up_time(Duration::from_secs(60));
    group.measurement_time(Duration::from_secs(120));

    // TODO: Fix bench
    // group.bench_function("nyoom", |b| {
    //     b.iter(|| {
    //         nyoom::Walker::new()
    //             .walk(Path::new(black_box("/usr")), |_path, is_dir| is_dir)
    //             .await
    //             .unwrap();
    //     })
    // });
    group.bench_function("ignore", |b| {
        b.iter(|| {
            ignore::WalkBuilder::new(black_box("/usr"))
                .threads(num_cpus::get())
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
    group.bench_function("jwalk", |b| {
        #[allow(unused_must_use)]
        b.iter(|| {
            for f in jwalk::WalkDir::new(black_box("/usr"))
                .parallelism(jwalk::Parallelism::RayonNewPool(num_cpus::get()))
                .sort(true)
            {
                black_box(f);
            }
        })
    });
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
