#[macro_use]
extern crate criterion;
use criterion::Criterion;
use tract_data::internal::*;

fn mat_mul_smmm(be: &mut criterion::Bencher, &(m, k, n): &(usize, usize, usize)) {
    unsafe {
        let mm = (tract_linalg::ops().mmm_f32)(m, k, n);
        let pa =
            Tensor::uninitialized_aligned::<f32>(&[mm.a_pack().len()], mm.a_pack().alignment())
                .unwrap();
        let pb =
            Tensor::uninitialized_aligned::<f32>(&[mm.b_pack().len()], mm.b_pack().alignment())
                .unwrap();
        let mut c = Tensor::zero::<f32>(&[m, n]).unwrap();
        be.iter(move || mm.run(&pa.view(), &pb.view(), &mut c.view_mut(), &[]));
    }
}

fn mat_mul_prepacked(c: &mut Criterion, m: usize, k: usize, n: usize) {
    c.bench_functions(
        &format!("mat_mul_prepacked"),
        vec![criterion::Fun::new("smmm", mat_mul_smmm)],
        (m, k, n),
    );
}

fn s64x288x21609(c: &mut Criterion) {
    mat_mul_prepacked(c, 64, 288, 21609)
}

criterion::criterion_group!(benches, s64x288x21609);
criterion::criterion_main!(benches);
