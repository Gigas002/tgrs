use super::*;
use crate::ui::tests::create_mock_media_message;

fn message(sender: i64, timestamp: i32) -> MessageEntry {
    MessageEntry::plain(1, TdMessageSender::User(sender), "text", timestamp)
}

// 2023-11-14 12:00 UTC: far from midnight in every time zone but a few
const NOON: i32 = 1_699_963_200;

#[test]
fn author_groups_break_on_sender_and_gaps() {
    let first = message(1, NOON);
    assert!(starts_author_group(None, &first));
    assert!(!starts_author_group(Some(&first), &message(1, NOON + 60)));
    assert!(starts_author_group(Some(&first), &message(2, NOON + 60)));
    let late = message(1, NOON + AUTHOR_GROUP_MAX_GAP_SECS);
    assert!(starts_author_group(Some(&first), &late));
    assert!(starts_new_day(Some(&first), &message(1, NOON + 86_400)));
    assert!(!starts_new_day(Some(&first), &message(1, NOON + 60)));
}

#[test]
fn attachments_are_labelled_with_name_and_size() {
    let entry = create_mock_media_message(1, "report.zip");
    assert_eq!(
        attachment_label(entry.media().unwrap()),
        "[file: report.zip] 2.0 KiB"
    );
}

#[test]
fn long_text_is_cut_with_an_ellipsis() {
    assert_eq!(truncate_display_width("short", 10), "short");
    assert_eq!(truncate_display_width("abcdefgh", 5), "abcd…");
    // Wide characters count as two columns
    assert_eq!(truncate_display_width("日本語テキスト", 7), "日本語…");
}

#[test]
fn links_mentions_and_code_use_their_theme_groups() {
    let theme = Theme::default();
    let body = ContentStyle::message_body(&theme);
    let link = TextStyle {
        link: true,
        ..TextStyle::default()
    };
    let mention = TextStyle {
        mention: true,
        ..TextStyle::default()
    };
    let code = TextStyle {
        code: true,
        ..TextStyle::default()
    };
    let expected = |group| {
        theme
            .style(HighlightGroup::MessageBody)
            .patch(theme.style(group))
    };
    assert_eq!(body.merge(link), expected(HighlightGroup::MessageLink));
    assert_eq!(body.merge(mention), expected(HighlightGroup::MentionOther));
    assert_eq!(body.merge(code), expected(HighlightGroup::InlineCode));

    // Previews keep their own color and only mark the text
    let preview = ContentStyle::from(Style::default());
    assert_eq!(
        preview.merge(link),
        Style::default().add_modifier(Modifier::UNDERLINED)
    );
}
