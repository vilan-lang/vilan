//! R8 with F1 M2 and F157: under a HOST, a termination ends the loop when the
//! stop COMPLETES inside a turn's poll while another source stays live
//! without holding the stop — the window's own shape, and the case the owned
//! loop once hung on (`termination_beside_a_live_source.rs`). The hosted turn
//! must answer `Finished::Stopped` rather than go idle waiting for a source
//! that holds nothing.
//!
//! A file of its own because the request is process-wide (see
//! `termination.rs`).

mod support;

use std::rc::Rc;

use support::{Host, on_fresh_thread};
use vilan_rt::executor::{
    Finished, Interest, IoSource, Program, pin_future, register_io, request_termination, sleep,
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
fn a_hosted_stop_that_completes_in_a_poll_ends_the_loop_beside_a_live_source() {
    let finished = on_fresh_thread(|| {
        let host = Host::with_sockets();
        let idle = host.idle_watch();
        let program = Program::start(async {
            let handler: Handler = Rc::new(|_request: Request, response: Response| {
                pin_future(async move { response.end("never asked") })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            register_io(Rc::new(LiveButNotHolding));
            sleep(30_000, None).await;
        });
        std::thread::spawn(move || {
            idle.until_idle(1);
            request_termination();
        });
        // The host's backstop is this pin's failure: a loop that went idle
        // beside the window after the stop completed is never woken again.
        host.drive(&program)
    });
    assert_eq!(finished, Finished::Stopped);
}
