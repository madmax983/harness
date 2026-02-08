//! Knowledge stream widget showing recent knowledge entries.

use harness_persistence::KnowledgeKind;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::AppState;

fn kind_style(kind: KnowledgeKind) -> Style {
    match kind {
        KnowledgeKind::Discovery => Style::default().fg(Color::Yellow),
        KnowledgeKind::Decision => Style::default().fg(Color::Blue),
        KnowledgeKind::Activity => Style::default().fg(Color::DarkGray),
        KnowledgeKind::Blocker => Style::default().fg(Color::Red),
    }
}

fn kind_label(kind: KnowledgeKind) -> &'static str {
    match kind {
        KnowledgeKind::Discovery => "Discovery",
        KnowledgeKind::Decision => "Decision",
        KnowledgeKind::Activity => "Activity",
        KnowledgeKind::Blocker => "Blocker",
    }
}

/// Render the knowledge stream panel.
pub fn render_knowledge(area: Rect, buf: &mut Buffer, state: &AppState) {
    let items: Vec<ListItem> = state
        .recent_knowledge
        .iter()
        .skip(state.knowledge_scroll)
        .map(|k| {
            let time = k.created_at.format("%H:%M");
            let line = Line::from(vec![
                Span::styled(format!("{time} "), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("[{}]", kind_label(k.kind)), kind_style(k.kind)),
                Span::raw(format!(" {}: ", k.author_id)),
                Span::raw(&k.content),
            ]);
            ListItem::new(line)
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Knowledge Stream ")
        .title_style(Style::default().fg(Color::Magenta));

    let list = List::new(items).block(block);

    Widget::render(list, area, buf);
}
