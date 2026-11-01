use super::*;
use crate::{app::action::Modifiers, ui::tests::create_test_app_context};
use crossterm::event::KeyModifiers;
use ratatui::{Terminal, backend::TestBackend};
use tokio::sync::mpsc;

fn report() -> (Report, mpsc::UnboundedReceiver<Action>) {
    let mut report = Report::new(create_test_app_context());
    let (tx, rx) = mpsc::unbounded_channel();
    report.register_action_handler(tx).unwrap();
    (report, rx)
}

fn sent(rx: &mut mpsc::UnboundedReceiver<Action>) -> Vec<Action> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn request(option_id: &str, text: &str) -> Action {
    Action::SendReport(ReportRequest {
        chat_id: 7,
        message_ids: vec![42],
        option_id: option_id.to_owned(),
        text: text.to_owned(),
    })
}

fn options() -> ReportStep {
    ReportStep::ChooseOption {
        title: "Why?".to_owned(),
        options: ["Spam", "Violence", "Other"]
            .iter()
            .enumerate()
            .map(|(i, text)| ReportOption {
                id: format!("id{i}"),
                text: (*text).to_owned(),
            })
            .collect(),
    }
}

fn key(code: KeyCode) -> Action {
    Action::Key(code, Modifiers::from(KeyModifiers::NONE))
}

fn screen(report: &mut Report) -> String {
    let (width, height) = (60, 12);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| report.draw(frame, frame.area()).unwrap())
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(usize::from(width))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_report_goes_from_reason_to_details_to_done() {
    let (mut report, mut rx) = report();
    report.update(Action::ShowReport(7, vec![42]));
    assert_eq!(sent(&mut rx), [request("", "")]);
    assert!(screen(&mut report).contains("Sending…"));
    assert!(screen(&mut report).contains("Report message"));

    report.update(Action::ReportStepped(options()));
    let shown = screen(&mut report);
    assert!(
        shown.contains("Why?") && shown.contains("Violence"),
        "{shown}"
    );
    report.update(Action::ReportNext);
    report.update(Action::ReportConfirm);
    assert_eq!(sent(&mut rx), [request("id1", "")]);

    report.update(Action::ReportStepped(ReportStep::WriteText {
        option_id: "id1".to_owned(),
        optional: false,
    }));
    // Details are required: enter on nothing does nothing
    report.update(Action::ReportConfirm);
    assert!(sent(&mut rx).is_empty());
    for c in "bad q".chars() {
        report.update(key(KeyCode::Char(c)));
    }
    report.update(Action::Paste("\\nstuff".into()));
    assert!(screen(&mut report).contains("Details: bad q"));
    report.update(Action::ReportConfirm);
    assert_eq!(sent(&mut rx), [request("id1", "bad q\\nstuff")]);

    report.update(Action::ReportStepped(ReportStep::Done));
    assert_eq!(sent(&mut rx), [Action::HideReport]);
}

#[test]
fn digits_pick_and_j_k_move_through_reasons() {
    let (mut report, mut rx) = report();
    report.update(Action::ShowReport(7, vec![42]));
    report.update(Action::ReportStepped(options()));
    sent(&mut rx);
    report.update(key(KeyCode::Char('k')));
    report.update(Action::ReportConfirm);
    assert_eq!(sent(&mut rx), [request("id2", "")], "k wraps to the last");

    report.update(Action::ReportStepped(options()));
    report.update(key(KeyCode::Char('j')));
    report.update(key(KeyCode::Char('9')));
    assert!(sent(&mut rx).is_empty(), "no ninth reason");
    report.update(key(KeyCode::Char('1')));
    assert_eq!(sent(&mut rx), [request("id0", "")]);
}

#[test]
fn optional_details_can_be_left_empty() {
    let (mut report, mut rx) = report();
    report.update(Action::ShowReport(7, vec![42]));
    report.update(Action::ReportStepped(ReportStep::WriteText {
        option_id: "x".to_owned(),
        optional: true,
    }));
    sent(&mut rx);
    assert!(screen(&mut report).contains("optional"));
    report.update(Action::ReportConfirm);
    assert_eq!(sent(&mut rx), [request("x", "")]);
}

#[test]
fn failures_and_chats_needing_messages_are_explained() {
    let (mut report, _rx) = report();
    report.update(Action::ShowReport(7, Vec::new()));
    assert!(screen(&mut report).contains("Report chat"));
    report.update(Action::ReportStepped(ReportStep::NeedsMessages));
    assert_eq!(report.stage, Stage::Failed(NEEDS_MESSAGES.to_owned()));
    report.update(Action::ReportFailed("Can't report: flood".into()));
    assert!(screen(&mut report).contains("Can't report: flood"));
}

#[test]
fn answers_after_closing_are_ignored() {
    let (mut report, mut rx) = report();
    report.update(Action::ShowReport(7, vec![42]));
    report.update(Action::HideReport);
    report.update(Action::ReportStepped(options()));
    sent(&mut rx);
    report.update(Action::ReportConfirm);
    assert!(sent(&mut rx).is_empty());
    assert!(screen(&mut report).trim().is_empty());
}
