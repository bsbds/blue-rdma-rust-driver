use blue_rdma_driver::TestDevice;

fn main() {
    TestDevice::init_emulated().unwrap();
}
