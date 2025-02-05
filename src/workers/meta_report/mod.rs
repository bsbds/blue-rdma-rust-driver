use crate::device_protocol::MetaReport;

struct MetaReportWorker<MRQ> {
    meta_report_queue: MRQ,
}

impl<MRQ: MetaReport> MetaReportWorker<MRQ> {
    fn run(self) {
        todo!()
    }
}
