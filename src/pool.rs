use std::sync::{Arc, Mutex, mpsc};

use eframe::egui::Color32;

use crate::{complex::fixed::*, sample};

pub(crate) struct Pool {
    threads: Vec<std::thread::JoinHandle<()>>,
    // sender: spmc::Sender<((Real, Imag), Color32)>,
    sender: mpsc::Sender<(Real, Imag)>,
    receiver: mpsc::Receiver<((Real, Imag), Color32)>,
    in_flight: usize,
}
impl Pool {
    pub(crate) fn new(thread_count: usize) -> Self {
        let (request_sender, request_receiver) = mpsc::channel();
        // emulate single producer multiple consumer
        let request_receiver = Arc::new(Mutex::new(request_receiver));
        let (response_sender, response_receiver) = mpsc::channel();
        let threads = (0..thread_count)
            .map(|_| {
                let request_receiver = request_receiver.clone();
                let response_sender = response_sender.clone();
                std::thread::spawn(move || {
                    while let Ok(receiver) = request_receiver.lock()
                        && let Ok(point) = receiver.recv()
                    {
                        let color = sample::metabrot_sample(point).color();
                        let Ok(_) = response_sender.send((point, color)) else {
                            break;
                        };
                    }
                })
            })
            .collect();
        Self {
            threads,
            sender: request_sender,
            receiver: response_receiver,
            in_flight: 0,
        }
    }
    pub(crate) fn in_flight(&self) -> usize {
        self.in_flight
    }
    pub(crate) fn thread_count(&self) -> usize {
        self.threads.len()
    }

    pub(crate) fn send(&mut self, point: (Real, Imag)) {
        self.sender.send(point).unwrap();
        self.in_flight += 1;
    }

    pub(crate) fn recv(&mut self) -> Option<((Real, Imag), Color32)> {
        if let Ok(ret) = self.receiver.try_recv() {
            assert!(self.in_flight > 0, "this is an invariant of the type");
            self.in_flight -= 1;
            Some(ret)
        } else {
            None
        }
    }
}
