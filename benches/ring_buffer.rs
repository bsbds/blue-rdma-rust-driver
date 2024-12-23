use blue_rdma_driver::bench_wrappers::{create_ring_wrapper, BenchDesc};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

#[allow(clippy::unit_arg)]
fn benchmark_ring_buffer_produce(c: &mut Criterion) {
    let mut ring = create_ring_wrapper();
    let descs = Some(BenchDesc::new([1; 32]));
    c.bench_function("ring produce", |b| {
        b.iter(|| black_box(ring.produce(descs.into_iter())))
    });
}

#[allow(clippy::unit_arg)]
fn benchmark_ring_buffer_consume(c: &mut Criterion) {
    let mut ring = create_ring_wrapper();
    c.bench_function("ring produce", |b| {
        b.iter(|| {
            let _ignore = black_box(ring.consume());
        })
    });
}

criterion_group!(
    benches,
    benchmark_ring_buffer_produce,
    benchmark_ring_buffer_consume
);
criterion_main!(benches);
