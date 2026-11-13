//! The program's side of the seam, without a window system: the event
//! decoder's three externs, a window source's order of work, the frame
//! builder's token and nesting, and text layout through the extern.

use std::cell::RefCell;
use std::rc::Rc;

use winit::event::{ElementState, MouseButton};
use winit::keyboard::ModifiersState;

use super::*;

/// A slot as `open` leaves it, for a window id the tests pick.
fn insert_window(id: u32) {
    STATE.with(|state| {
        state.borrow_mut().windows.insert(
            id,
            WindowSlot {
                title: String::new(),
                requested: (320.0, 200.0),
                background: Color::WHITE,
                window: None,
                answer: None,
                resolve: None,
                device_size: (0, 0),
                scale_factor: 1.0,
                pointer: (0.0, 0.0),
                modifiers: ModifiersState::empty(),
                on_frame: None,
                on_event: None,
                ui_root: None,
                last_draw_us: 0.0,
                redraw_requests: 0,
                events: VecDeque::new(),
                frame_due: false,
                scene: None,
                closing: false,
                access_requests: VecDeque::new(),
            },
        )
    });
}

fn queue(id: u32, event: RawEvent) {
    with_window(id, |slot| input::enqueue(&mut slot.events, event));
}

#[test]
fn events_are_read_one_at_a_time_through_the_three_externs() {
    insert_window(7);
    queue(
        7,
        RawEvent::pointer_button(
            ElementState::Pressed,
            12.5,
            40.0,
            MouseButton::Left,
            ModifiersState::CONTROL,
        ),
    );
    queue(7, RawEvent::text_input("é".to_string()));
    assert_eq!(event_next(7), input::POINTER_PRESSED);
    assert_eq!(
        (0..4).map(event_number).collect::<Vec<_>>(),
        vec![12.5, 40.0, 0.0, 2.0]
    );
    assert_eq!(&*event_text(), "");
    assert_eq!(event_next(7), input::TEXT_INPUT);
    assert_eq!(&*event_text(), "é");
    assert_eq!(event_next(7), -1);
    assert_eq!(event_next(99), -1, "an unknown window has no events");
}

#[test]
fn a_window_source_answers_its_open_then_hands_over_its_events() {
    insert_window(3);
    let seen: Rc<RefCell<Vec<String>>> = Rc::default();
    let answered = Rc::clone(&seen);
    let listened = Rc::clone(&seen);
    with_window(3, |slot| {
        slot.answer = Some(Ok(3));
        slot.resolve = Some(Rc::new(move |answer: Result<u32, Str>| {
            answered.borrow_mut().push(format!("answer {answer:?}"))
        }));
        slot.on_event = Some(Rc::new(move || {
            while event_next(3) >= 0 {
                listened
                    .borrow_mut()
                    .push(format!("event at {}", event_number(0)));
            }
        }));
    });
    queue(3, RawEvent::pointer_moved(5.0, 6.0));
    let source = WindowSource { id: 3 };
    assert!(source.poll(), "the answer is the first work");
    assert!(source.poll(), "the events are the second");
    assert!(!source.poll(), "then nothing: no window exists to draw");
    assert_eq!(*seen.borrow(), vec!["answer Ok(3)", "event at 5"]);
    assert!(source.is_live());
}

#[test]
fn a_window_that_could_not_be_made_answers_and_its_source_ends() {
    insert_window(4);
    let answer: Rc<RefCell<Option<Result<u32, Str>>>> = Rc::default();
    let kept = Rc::clone(&answer);
    with_window(4, |slot| {
        slot.answer = Some(Err(str_new("no adapter")));
        slot.resolve = Some(Rc::new(move |given| *kept.borrow_mut() = Some(given)));
    });
    let source = WindowSource { id: 4 };
    assert!(source.poll());
    assert_eq!(*answer.borrow(), Some(Err(str_new("no adapter"))));
    assert!(!source.is_live(), "a window never made leaves no source");
}

#[test]
fn events_with_nobody_listening_are_dropped() {
    insert_window(5);
    queue(5, RawEvent::pointer_left());
    let source = WindowSource { id: 5 };
    assert!(!source.poll());
    assert_eq!(event_next(5), -1);
}

#[test]
fn a_frame_names_its_token_size_and_scale_and_builds_its_scene() {
    let drawn = build_frame(
        (320.0, 200.0),
        1.5,
        Some(&|| {
            let token = frame_number(0) as u32;
            assert_eq!(frame_number(1), 320.0);
            assert_eq!(frame_number(2), 200.0);
            assert_eq!(frame_number(3), 1.5);
            rectangle(
                token, 0.0, 0.0, 10.0, 10.0, 0.0, 0.0, 0.0, 0.0, 0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            );
            push_layer(token, 0.5);
            shadow(token, 0.0, 0.0, 4.0, 4.0, 1.0, 2.0, 0.0, 0.0, 0.0, 0.5);
            pop(token);
        }),
    );
    assert_eq!(drawn.primitive_count(), 2);
}

#[test]
fn a_frame_drawn_inside_another_keeps_the_outer_one() {
    let outer = build_frame(
        (10.0, 10.0),
        1.0,
        Some(&|| {
            let token = frame_number(0) as u32;
            let inner = build_frame(
                (5.0, 5.0),
                2.0,
                Some(&|| {
                    let inner_token = frame_number(0) as u32;
                    underline(inner_token, 0.0, 0.0, 5.0, 1.0, 0.0, 0.0, 0.0, 1.0);
                }),
            );
            assert_eq!(inner.primitive_count(), 1);
            assert_eq!(frame_number(1), 10.0, "the outer frame is back");
            underline(token, 0.0, 0.0, 5.0, 1.0, 0.0, 0.0, 0.0, 1.0);
            underline(token, 0.0, 2.0, 5.0, 1.0, 0.0, 0.0, 0.0, 1.0);
        }),
    );
    assert_eq!(outer.primitive_count(), 2);
}

#[test]
fn drawing_into_a_frame_after_its_callback_returned_fails_the_program() {
    let token: Rc<RefCell<u32>> = Rc::default();
    let kept = Rc::clone(&token);
    build_frame(
        (1.0, 1.0),
        1.0,
        Some(&move || *kept.borrow_mut() = frame_number(0) as u32),
    );
    let stale = *token.borrow();
    let failure = std::panic::catch_unwind(|| pop(stale)).expect_err("a stale frame fails");
    let message = failure.downcast_ref::<String>().expect("a message");
    assert!(
        message.contains("after its frame callback returned"),
        "{message}"
    );
}

#[test]
fn an_unbalanced_frame_fails_with_the_scenes_reason() {
    let failure = std::panic::catch_unwind(|| {
        build_frame(
            (1.0, 1.0),
            1.0,
            Some(&|| push_layer(frame_number(0) as u32, 1.0)),
        )
    })
    .expect_err("an open layer at the end fails");
    let message = failure.downcast_ref::<String>().expect("a message");
    assert!(message.contains("never popped"), "{message}");
}

#[test]
fn a_window_reports_the_size_it_asked_for_until_it_exists() {
    insert_window(6);
    assert_eq!(window_number(6, 0), 320.0);
    assert_eq!(window_number(6, 1), 200.0);
    assert_eq!(window_number(6, 2), 1.0);
    assert_eq!(window_number(600, 0), 0.0, "a closed window answers 0");
}

#[test]
fn opening_without_the_window_system_answers_why() {
    let task = open(&str_new("t"), 100.0, 100.0, 1.0, 1.0, 1.0, 1.0);
    assert!(task.is_settled(), "the refusal is immediate");
}

// ------------------------------------------------------------- std::ui ---

fn redraws(id: u32) -> u64 {
    with_window(id, |slot| slot.redraw_requests).expect("the window")
}

/// native-ui.md R9: a change to an element of a MOUNTED tree asks its window
/// for a frame; a change to an element in no mounted tree, and a write that
/// changes nothing, do not.
#[test]
fn a_change_to_a_mounted_tree_asks_its_window_for_a_frame_and_nothing_else_does() {
    insert_window(11);
    let root = ui::node(1);
    let label = ui::node(3);
    let run = ui::append_text(&label, &str_new("0"));
    ui::add_child(&root, &label);
    ui::mount(11, &root);
    let mounted = redraws(11);
    ui::set_run(&label, run, &str_new("1"));
    assert_eq!(redraws(11), mounted + 1, "a run of a mounted text");
    ui::set_run(&label, run, &str_new("1"));
    assert_eq!(redraws(11), mounted + 1, "the same text: no change");
    let elsewhere = ui::node(0);
    ui::set(&elsewhere, 2, 8.0);
    assert_eq!(redraws(11), mounted + 1, "an element in no mounted tree");
    let group = ui::style_group(&root, -1);
    let layer = ui::style_layer(&root, group, false, 0, -1.0, -1.0);
    ui::style_color(&root, layer, 0, 1.0, 0.0, 0.0, 1.0);
    ui::style_end(&root);
    assert_eq!(redraws(11), mounted + 2, "a look");
}

/// A key handler that stops its event: the ancestor listening never hears
/// it, and Tab does not move the focus.
#[test]
fn a_stopped_event_bubbles_no_further_and_its_default_does_not_happen() {
    let root = ui::node(1);
    let first = ui::node(0);
    let second = ui::node(0);
    ui::add_child(&root, &first);
    ui::add_child(&root, &second);
    ui::set_focusable(&first, true);
    ui::set_focusable(&second, true);
    ui::layout(&root, 100.0, 100.0, 1.0);
    let heard = Rc::new(RefCell::new(Vec::new()));
    let stopping = Rc::new(std::cell::Cell::new(true));
    {
        let heard = Rc::clone(&heard);
        let stopping = Rc::clone(&stopping);
        ui::on(
            &first,
            7,
            Rc::new(move || {
                heard.borrow_mut().push("first");
                if stopping.get() {
                    ui::stop();
                }
            }),
        );
    }
    {
        let heard = Rc::clone(&heard);
        ui::on(&root, 7, Rc::new(move || heard.borrow_mut().push("root")));
    }
    ui::focus(&first);
    let tab = |shift: f64| {
        ui::deliver(
            &root,
            input::KEY_PRESSED,
            0.0,
            shift,
            1.0,
            0.0,
            0.0,
            &str_new("Tab"),
        )
    };
    tab(0.0);
    assert_eq!(*heard.borrow(), ["first"], "stopped at the focus");
    assert_eq!(ui::interaction(&first) & 4, 4, "Tab did not move the focus");
    stopping.set(false);
    heard.borrow_mut().clear();
    tab(0.0);
    assert_eq!(*heard.borrow(), ["first", "root"], "bubbled to the root");
    assert_eq!(ui::interaction(&second) & 4, 4, "and Tab moved the focus");
}

/// A handler reads its event through `std::gfx`'s current event: the window
/// event being routed while it runs, and none after.
#[test]
fn a_handler_runs_with_the_routed_event_as_the_current_one() {
    let root = ui::node(1);
    ui::set_length(&root, 0, 1, 50.0);
    ui::set_length(&root, 1, 1, 50.0);
    ui::layout(&root, 100.0, 100.0, 1.0);
    let seen = Rc::new(RefCell::new(Vec::new()));
    {
        let seen = Rc::clone(&seen);
        ui::on(
            &root,
            6,
            Rc::new(move || {
                seen.borrow_mut().push((event_current(), event_number(3)));
            }),
        );
    }
    ui::deliver(
        &root,
        input::SCROLLED,
        10.0,
        10.0,
        0.0,
        -32.0,
        0.0,
        &str_new(""),
    );
    assert_eq!(*seen.borrow(), [(input::SCROLLED, -32.0)]);
    assert_eq!(event_current(), -1, "none after");
}

/// Counts its drops: what a handler that must be dropped captures.
struct DropCount(Rc<std::cell::Cell<u32>>);

impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

/// B738: a handler that captures its own element is a cycle — the element
/// holds the handler, the handler holds the element's handle — so the
/// element outlived every handle the program had and lived until the program
/// ended. Removing the element drops its subtree's handlers, after the tree
/// is released (a handle's drop releases its element in the tree): the
/// cycle's last handle goes, and the element is freed with its child.
#[test]
fn a_removed_element_drops_its_handlers_and_a_handler_holding_it_lets_it_go() {
    let root = ui::node(1);
    let alive = ui::element_count();
    let row = ui::node(0);
    let label = ui::node(3);
    ui::add_child(&row, &label);
    ui::add_child(&root, &row);
    let drops = Rc::new(std::cell::Cell::new(0));
    // The row's handler holds the row (`on_click(|| row.focus())`), its
    // child's holds the child, each with a drop counter beside it.
    for (element, kind) in [(&row, 5), (&label, 10)] {
        let held = element.clone();
        let counted = DropCount(Rc::clone(&drops));
        ui::on(
            element,
            kind,
            Rc::new(move || {
                let _ = (&held, &counted);
            }),
        );
    }
    drop(label);
    assert_eq!(ui::element_count(), alive + 2);
    ui::remove(&row);
    assert_eq!(drops.get(), 2, "both handlers dropped");
    assert_eq!(
        ui::element_count(),
        alive + 2,
        "the program still holds the row"
    );
    drop(row);
    assert_eq!(
        ui::element_count(),
        alive,
        "the row and its child are freed"
    );
    let _ = root;
}

/// F169: after a present the X11 loop wakes on its own, on a schedule that
/// starts at the present, passes each step it reaches and ends; nothing
/// before the first present wakes it, and a new present starts the schedule
/// again from the top.
#[test]
fn a_present_schedules_bounded_wakes_and_then_the_loop_sleeps() {
    use std::time::{Duration, Instant};

    use super::shell::PresentDrain;

    let start = Instant::now();
    let mut drain = PresentDrain::default();
    assert_eq!(drain.next_wake(start), None, "no present, no wake");

    drain.presented(start);
    let mut now = start;
    let mut wakes = Vec::new();
    while let Some(wake) = drain.next_wake(now) {
        assert!(wake > now, "a wake is always ahead of now");
        wakes.push(wake - start);
        now = wake;
    }
    assert_eq!(wakes, PresentDrain::STEPS.to_vec(), "every step, in order");
    assert!(
        *wakes.last().expect("a step") <= Duration::from_secs(3),
        "the schedule ends within seconds of the present"
    );
    assert_eq!(drain.next_wake(now + Duration::from_secs(60)), None);

    // A wait that overslept several steps resumes at the first still ahead.
    drain.presented(start);
    assert_eq!(
        drain.next_wake(start + Duration::from_millis(100)),
        Some(start + Duration::from_millis(128))
    );
    // A present starts the schedule again from its first step.
    let later = start + Duration::from_millis(300);
    drain.presented(later);
    assert_eq!(drain.next_wake(later), Some(later + PresentDrain::STEPS[0]));
}

/// F169: the loop waits until the earlier of the program's deadline and the
/// next drain wake, and for an event alone when there is neither — never
/// `Poll`.
#[test]
fn the_loop_waits_for_the_earlier_of_the_deadline_and_the_drain_wake() {
    use std::time::{Duration, Instant};

    use winit::event_loop::ControlFlow;

    use super::shell::wait_for;

    let now = Instant::now();
    let soon = now + Duration::from_millis(16);
    let later = now + Duration::from_millis(500);
    assert_eq!(wait_for(None, None), ControlFlow::Wait);
    assert_eq!(wait_for(Some(later), None), ControlFlow::WaitUntil(later));
    assert_eq!(wait_for(None, Some(soon)), ControlFlow::WaitUntil(soon));
    assert_eq!(
        wait_for(Some(later), Some(soon)),
        ControlFlow::WaitUntil(soon)
    );
    assert_eq!(
        wait_for(Some(soon), Some(later)),
        ControlFlow::WaitUntil(soon)
    );
}
