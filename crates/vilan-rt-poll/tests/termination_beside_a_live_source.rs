//! F1 M2 with F157: a termination ends the loop when the stop COMPLETES inside
//! a turn's poll while another source stays live without holding the stop.
//!
//! A desktop window is such a source (live while the window is open, holding
//! nothing), and so is stdin's reader. The server held the stop when the turn
//! began; its poll then closed its listener and left the registry; the loop
//! must not go on to block in the wait for the window, which nothing would
//! ever wake. Before the fix the loop hung there: a desktop program ignored
//! SIGTERM while its window was open.
//!
//! A file of its own because the request is process-wide (see `termination.rs`).

use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use vilan_rt::executor::{
    Interest, IoSource, TerminationStop, block_on, register_io, request_termination, sleep,
};
use vilan_rt::http::{Handler, Request, Response, create_server};

/// Live for good, holding nothing, describing itself completely and waiting
/// on nothing the loop could watch — a window's shape.
struct LiveButNotHolding;

impl IoSource for LiveButNotHolding {
    fn poll(&self) -> bool {
        false
    }

    fn interest(&self, _interest: &mut Interest) -> bool {
        true
    }

    fn is_live(&self) -> bool {
        true
    }
}

#[test]
fn a_stop_that_completes_in_a_poll_ends_the_loop_beside_a_live_source() {
    let (ended, outcome) = mpsc::channel();
    std::thread::spawn(move || {
        vilan_rt_poll::install();
        let result = std::panic::catch_unwind(|| {
            block_on(async {
                let handler: Handler = Rc::new(|_request: Request, response: Response| {
                    vilan_rt::executor::pin_future(async move {
                        response.end("never asked");
                    })
                });
                let server = create_server(handler);
                server.listen(0, Rc::new(|| {}));
                register_io(Rc::new(LiveButNotHolding));
                std::thread::spawn(|| {
                    std::thread::sleep(Duration::from_millis(100));
                    request_termination();
                });
                sleep(30_000, None).await;
            });
        });
        let stopped = result
            .err()
            .is_some_and(|payload| payload.downcast_ref::<TerminationStop>().is_some());
        let _ = ended.send(stopped);
    });
    match outcome.recv_timeout(Duration::from_secs(10)) {
        Ok(stopped) => assert!(stopped, "the loop ended, but not by the stop"),
        Err(_) => panic!(
            "the loop never ended: the stop completed in a poll and the loop went back to \
             waiting for a source that holds nothing"
        ),
    }
}
