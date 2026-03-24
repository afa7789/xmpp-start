// L4: Ad-hoc commands UI — XEP-0050
//
// Flow:
//   1. User opens panel → app sends XmppCommand::DiscoverAdhocCommands { target_jid }
//   2. Engine returns XmppEvent::AdhocCommandsDiscovered { from_jid, commands }
//      → populate command list
//   3. User selects a command → app sends XmppCommand::ExecuteAdhocCommand { to_jid, node }
//   4. Engine returns XmppEvent::AdhocCommandResult(CommandResponse)
//      → show response form (using XEP-0004 DataForm renderer)
//   5. User fills form and submits → app sends XmppCommand::ContinueAdhocCommand
//   6. Or user cancels → app sends XmppCommand::CancelAdhocCommand

use iced::{
    widget::{button, column, container, row, scrollable, text, text_input},
    Alignment, Element, Length, Task,
};

use crate::xmpp::modules::adhoc::{CommandResponse, CommandStatus, DataField};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Which step of the ad-hoc flow we are in.
#[derive(Debug, Clone, PartialEq)]
pub enum AdhocStep {
    /// Showing the server JID input and waiting for user to confirm discovery.
    TargetInput,
    /// Waiting for discovery results to arrive.
    Discovering,
    /// Command list is shown; user has not yet selected one.
    CommandList,
    /// A command was selected and executed; we are waiting for the result.
    Executing,
    /// Showing a form returned by the server.
    ShowingForm(CommandResponse),
    /// Command completed.
    Done(String),
}

#[derive(Debug, Clone)]
pub struct AdhocScreen {
    /// JID of the server/service to discover commands on.
    pub target_jid: String,
    /// Available commands: (node, name).
    pub commands: Vec<(String, String)>,
    /// Current step.
    pub step: AdhocStep,
    /// Form field values (keyed by `var`).
    pub field_values: std::collections::HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Message {
    /// User typed in the target JID field.
    TargetJidChanged(String),
    /// User clicked "Discover".
    DiscoverRequested,
    /// Engine returned the command list.
    CommandsDiscovered { _from_jid: String, commands: Vec<(String, String)> },
    /// User clicked on a command item.
    CommandSelected(String),
    /// Engine returned a command response.
    CommandResponseReceived(CommandResponse),
    /// User changed a form field.
    FieldChanged(String, String),
    /// User clicked "Submit" on the form.
    SubmitForm,
    /// User clicked "Cancel" on the form.
    CancelCommand,
    /// User clicked "Close" / back.
    Close,
    /// User clicked "Back to list" after a command completes.
    BackToList,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl Default for AdhocScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl AdhocScreen {
    pub fn new() -> Self {
        Self {
            target_jid: String::new(),
            commands: vec![],
            step: AdhocStep::TargetInput,
            field_values: std::collections::HashMap::new(),
        }
    }

    /// The node of the currently-executing command (if any).
    pub fn active_node(&self) -> Option<&str> {
        match &self.step {
            AdhocStep::ShowingForm(resp) => Some(&resp.node),
            _ => None,
        }
    }

    /// The session_id of the current command response (if any).
    pub fn active_session_id(&self) -> Option<&str> {
        match &self.step {
            AdhocStep::ShowingForm(resp) => Some(&resp.session_id),
            _ => None,
        }
    }

    /// Collect the current form fields as DataField snapshots for submission.
    pub fn collect_fields(&self) -> Vec<DataField> {
        match &self.step {
            AdhocStep::ShowingForm(resp) => resp
                .fields
                .iter()
                .map(|f| DataField {
                    var: f.var.clone(),
                    label: f.label.clone(),
                    field_type: f.field_type.clone(),
                    value: self.field_values.get(&f.var).cloned().or(f.value.clone()),
                    options: f.options.clone(),
                })
                .collect(),
            _ => vec![],
        }
    }

    pub fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::TargetJidChanged(v) => {
                self.target_jid = v;
            }
            Message::DiscoverRequested => {
                self.step = AdhocStep::Discovering;
                self.commands.clear();
                // Caller intercepts to send XmppCommand::DiscoverAdhocCommands.
            }
            Message::CommandsDiscovered { _from_jid: _, commands } => {
                self.commands = commands;
                self.step = AdhocStep::CommandList;
            }
            Message::CommandSelected(_node) => {
                self.step = AdhocStep::Executing;
                self.field_values.clear();
                // Caller intercepts to send XmppCommand::ExecuteAdhocCommand.
            }
            Message::CommandResponseReceived(resp) => match resp.status {
                CommandStatus::Completed => {
                    let notes = resp.notes.join("; ");
                    let summary = if notes.is_empty() {
                        "Command completed.".into()
                    } else {
                        notes
                    };
                    self.step = AdhocStep::Done(summary);
                }
                CommandStatus::Canceled => {
                    self.step = AdhocStep::CommandList;
                }
                CommandStatus::Executing => {
                    // Pre-populate field_values from server defaults.
                    self.field_values.clear();
                    for f in &resp.fields {
                        if let Some(ref v) = f.value {
                            self.field_values.insert(f.var.clone(), v.clone());
                        }
                    }
                    self.step = AdhocStep::ShowingForm(resp);
                }
            },
            Message::FieldChanged(var, val) => {
                self.field_values.insert(var, val);
            }
            Message::SubmitForm => {
                // Caller intercepts to send XmppCommand::ContinueAdhocCommand.
            }
            Message::CancelCommand => {
                // Caller intercepts to send XmppCommand::CancelAdhocCommand.
                self.step = AdhocStep::CommandList;
                self.field_values.clear();
            }
            Message::Close => {
                // Caller handles navigation.
            }
            Message::BackToList => {
                self.step = AdhocStep::CommandList;
                self.field_values.clear();
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let title = text("Ad-Hoc Commands (XEP-0050)").size(20);
        let close_btn = button("Close").on_press(Message::Close).padding([4, 12]);

        let body: Element<'_, Message> = match &self.step {
            AdhocStep::TargetInput => self.view_target_input(),
            AdhocStep::Discovering => text("Discovering commands…").into(),
            AdhocStep::CommandList => self.view_command_list(),
            AdhocStep::Executing => text("Executing command…").into(),
            AdhocStep::ShowingForm(resp) => self.view_form(resp),
            AdhocStep::Done(msg) => column![
                text(msg.as_str()),
                button("Back to list").on_press(Message::BackToList).padding([4, 12]),
            ]
            .spacing(8)
            .into(),
        };

        let content = column![
            row![title, close_btn]
                .spacing(8)
                .align_y(Alignment::Center),
            body,
        ]
        .spacing(16)
        .padding(20);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_target_input(&self) -> Element<'_, Message> {
        let jid_input = text_input("admin@example.com or server.example.com", &self.target_jid)
            .on_input(Message::TargetJidChanged)
            .on_submit(Message::DiscoverRequested)
            .padding(6)
            .width(Length::Fixed(320.0));
        let discover_btn = button("Discover")
            .on_press(Message::DiscoverRequested)
            .padding([6, 12]);
        column![
            text("Enter the JID of the server or service:"),
            row![jid_input, discover_btn].spacing(8),
        ]
        .spacing(8)
        .into()
    }

    fn view_command_list(&self) -> Element<'_, Message> {
        if self.commands.is_empty() {
            return column![
                text("No commands found on this service."),
                button("Try again")
                    .on_press(Message::DiscoverRequested)
                    .padding([4, 12]),
            ]
            .spacing(8)
            .into();
        }

        let items: Vec<Element<'_, Message>> = self
            .commands
            .iter()
            .map(|(node, name)| {
                let label = if name.is_empty() { node.as_str() } else { name.as_str() };
                button(text(label))
                    .on_press(Message::CommandSelected(node.clone()))
                    .width(Length::Fill)
                    .padding([6, 12])
                    .into()
            })
            .collect();

        let list = scrollable(
            column(items).spacing(4).width(Length::Fill),
        )
        .height(Length::Fixed(300.0));

        column![
            text(format!("{} command(s) available:", self.commands.len())),
            list,
        ]
        .spacing(8)
        .into()
    }

    fn view_form<'a>(&self, resp: &'a CommandResponse) -> Element<'a, Message> {
        let mut form_rows: Vec<Element<'a, Message>> = Vec::new();

        for field in &resp.fields {
            // Skip hidden fields.
            if field.field_type == "hidden" {
                continue;
            }
            let label_text = field
                .label
                .as_deref()
                .unwrap_or(field.var.as_str());
            let current_val = self
                .field_values
                .get(&field.var)
                .cloned()
                .unwrap_or_default();
            let var = field.var.clone();
            let input = text_input(label_text, &current_val)
                .on_input(move |v| Message::FieldChanged(var.clone(), v))
                .padding(6)
                .width(Length::Fixed(280.0));
            form_rows.push(
                row![
                    text(label_text).width(Length::Fixed(120.0)),
                    input,
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            );
        }

        // Notes from the server.
        for note in &resp.notes {
            form_rows.push(text(note.as_str()).size(12).into());
        }

        let submit_btn = button("Submit")
            .on_press(Message::SubmitForm)
            .padding([6, 12]);
        let cancel_btn = button("Cancel")
            .on_press(Message::CancelCommand)
            .padding([6, 12]);

        let mut col = column(form_rows).spacing(8);
        col = col.push(row![submit_btn, cancel_btn].spacing(8));

        scrollable(col).height(Length::Fixed(400.0)).into()
    }
}
