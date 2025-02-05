use crate::device_protocol::WorkReqSend;

struct SendWorker<SQ> {
    send_queue: SQ,
    inject: Inject,
}

struct Inject;

struct Task;

impl<SQ: WorkReqSend> SendWorker<SQ> {
    fn run(self) {
        loop {
            let task = self.find_task();
            self.process(task);
        }
    }

    fn find_task(&self) -> Task {
        todo!()
    }

    fn process(&self, task: Task) {
        todo!()
    }
}
