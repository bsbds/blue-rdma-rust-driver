use super::{constants::CSR_DEVICE_MODE_ADDR, DeviceAdaptor};

#[derive(Default, Clone, Copy)]
pub(crate) enum Mode {
    #[default]
    Mode400G,
    Mode200G,
    Mode100G,
}

impl Mode {
    pub(crate) const fn num_channel(self) -> usize {
        match self {
            Mode::Mode100G => 1,
            Mode::Mode200G => 2,
            Mode::Mode400G => 4,
        }
    }

    pub(crate) const fn channel_ids(self) -> &'static [usize] {
        match self {
            Mode::Mode100G => &[0],
            Mode::Mode200G => &[0, 1],
            Mode::Mode400G => &[0, 1, 2, 3],
        }
    }
}

struct ModeProxy<Dev> {
    dev: Dev,
}

impl<Dev: DeviceAdaptor> ModeProxy<Dev> {
    pub(crate) fn mode(&self) -> Mode {
        let mode = self
            .dev
            .read_csr(CSR_DEVICE_MODE_ADDR)
            .unwrap_or_else(|_| unreachable!("failed to read mode from device"));
        match mode {
            0 => Mode::Mode100G,
            1 => Mode::Mode200G,
            2 => Mode::Mode400G,
            _ => unreachable!("invalid mode"),
        }
    }
}
