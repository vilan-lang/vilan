//! R8 with F45/F157: a termination request reaches a HOSTED loop asleep in
//! its host. The program serves (the poller waits on its listener) with a far
//! timer behind it; the request comes from another thread once the host is
//! asleep, and only the host's wake can end that sleep. The loop answers
//! `Finished::Stopped`, and `main`, still suspended, ends with the stop.
//!
//! A file of its own because the request is process-wide (see
//! `termination.rs`).

mod support;

use std::rc::Rc;
use std::time::{Duration, Instant};

use support::{Host, on_fresh_thread};
use vilan_rt::executor::{
    Finished, Program, TerminationStop, pin_future, request_termination, sleep,
};
use vilan_rt::http::{Handler, Request, Response, create_server};

#[test]
fn a_termination_request_stops_a_hosted_loop_asleep_in_its_host() {
    let started = Instant::now();
    let (finished, stopped, wakes) = on_fresh_thread(|| {
        let host = Host::with_sockets();
        let idle = host.idle_watch();
        let program = Program::start(async {
            let handler: Handler = Rc::new(|_request: Request, response: Response| {
                pin_future(async move { response.end("never asked") })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            // `main` stays suspended far past the request: the stop ends it.
            sleep(30_000, None).await;
        });
        std::thread::spawn(move || {
            idle.until_idle(1);
            request_termination();
        });
        let finished = host.drive(&program);
        let stopped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| program.finish()))
            .err()
            .is_some_and(|payload| payload.downcast_ref::<TerminationStop>().is_some());
        (finished, stopped, host.wakes())
    });
    let elapsed = started.elapsed();
    assert_eq!(finished, Finished::Stopped);
    assert!(stopped, "`main` was still suspended: it ends with the stop");
    assert!(wakes >= 1, "the request reached the host through its wake");
    assert!(
        elapsed < Duration::from_secs(5),
        "the loop ended {elapsed:?} after it began: the request never woke the host"
    );
}
