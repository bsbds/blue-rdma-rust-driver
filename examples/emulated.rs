use blue_rdma_driver::EmulatedDevice;

bluesimalloc::setup_allocator!();

fn main() {
    EmulatedDevice::run("127.0.0.1:7700".parse().unwrap()).unwrap();
}
