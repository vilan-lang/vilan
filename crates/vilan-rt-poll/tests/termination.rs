//! F160 with F45/F157: a termination request reaches a loop BLOCKED in its
//! readiness wait. The request comes from another thread (the signal
//! handler's, in a program), and the loop is waiting on an idle server's
//! listener with a far timer behind it — before the bell, only the next
//! tick would have seen the request; now the wait itself must end.
//!
//! A file of its own because the request is process-wide: under `cargo
//! test`, a binary's tests share the process, and every other loop in it
//! would stop too.

use std::rc::Rc;
use std::time::{Duration, Instant};

use vilan_rt::executor::{TerminationStop, block_on, request_termination, sleep};
use vilan_rt::http::{Handler, Request, Response, create_server};

#[test]
fn a_termination_request_ends_a_loop_blocked_in_its_readiness_wait() {
    vilan_rt_poll::install();
    let started = Instant::now();
    let ended = std::panic::catch_unwind(|| {
        block_on(async {
            let handler: Handler = Rc::new(|_request: Request, response: Response| {
                vilan_rt::executor::pin_future(async move {
                    response.end("never asked");
                })
            });
            let server = create_server(handler);
            server.listen(0, Rc::new(|| {}));
            std::thread::spawn(|| {
                std::thread::sleep(Duration::from_millis(100));
                request_termination();
            });
            // `main` stays suspended far past the request: the stop ends it.
            sleep(30_000, None).await;
        });
    });
    let elapsed = started.elapsed();
    let payload = ended.expect_err("a stop with `main` still suspended unwinds out of `block_on`");
    assert!(
        payload.downcast_ref::<TerminationStop>().is_some(),
        "the unwind is the stop's"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "the loop ended {elapsed:?} after it began: the request never woke the wait"
    );
}
