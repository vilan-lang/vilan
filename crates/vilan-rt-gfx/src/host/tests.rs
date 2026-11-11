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
                events: VecDeque::new(),
                frame_due: false,
                scene: None,
                closing: false,
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
