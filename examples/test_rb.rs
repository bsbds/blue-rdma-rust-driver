use blue_rdma_driver::TestDevice;

fn main() {
    let iter = std::env::args()
        .nth(1)
        .and_then(|x| x.parse().ok())
        .unwrap_or(4096);
    TestDevice::test_rb(iter).unwrap();
}
