#![allow(dead_code)]
// Task P6.2 — XEP-0050 Ad-Hoc Commands
// XEP reference: https://xmpp.org/extensions/xep-0050.html
//
// Pure state machine — no I/O, no async.
// Builds and parses command IQs (execute, next, cancel, result).

use std::collections::HashMap;

use tokio_xmpp::minidom::Element;
use uuid::Uuid;

use super::{NS_CLIENT, NS_DATA};

const NS_ADHOC: &str = "http://jabber.org/protocol/commands";

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Status of an ad-hoc command session.
#[derive(Debug, Clone, PartialEq)]
pub enum CommandStatus {
    Executing,
    Completed,
    Canceled,
}

/// A single field in an x-data form.
#[derive(Debug, Clone, PartialEq)]
pub struct DataField {
    pub var: String,
    pub label: Option<String>,
    /// e.g. `"text-single"`, `"boolean"`, `"list-single"`.
    pub field_type: String,
    pub value: Option<String>,
    /// `(label, value)` pairs for `list-single` / `list-multi` fields.
    pub options: Vec<(String, String)>,
}

/// Parsed result from a command IQ response.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandResponse {
    pub session_id: String,
    pub node: String,
    pub status: CommandStatus,
    pub fields: Vec<DataField>,
    pub notes: Vec<String>,
}

// ---------------------------------------------------------------------------
// AdhocManager
// ---------------------------------------------------------------------------

/// XEP-0050 Ad-Hoc Commands state manager.
///
/// Builds outbound command IQs and parses inbound command result IQs.
///
/// All methods are pure: no I/O, no async.
pub struct AdhocManager {
    /// Pending command requests: `iq_id` → `node`.
    pending: HashMap<String, String>,
}

impl AdhocManager {
    /// Create a new manager with no pending requests.
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
        }
    }

    /// Build an IQ to execute a command (initial request, no session).
    ///
    /// ```xml
    /// <iq type="set" id="{id}" to="{jid}">
    ///   <command xmlns="http://jabber.org/protocol/commands" action="execute" node="{node}"/>
    /// </iq>
    /// ```
    ///
    /// Returns `(iq_id, element)`.
    pub fn build_execute(&mut self, to_jid: &str, node: &str) -> (String, Element) {
        let id = Uuid::new_v4().to_string();
        let command = Element::builder("command", NS_ADHOC)
            .attr("action", "execute")
            .attr("node", node)
            .build();
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("id", &id)
            .attr("to", to_jid)
            .append(command)
            .build();
        self.pending.insert(id.clone(), node.to_string());
        (id, iq)
    }

    /// Build an IQ to continue a command session with filled-in fields.
    ///
    /// ```xml
    /// <iq type="set" id="{id}" to="{jid}">
    ///   <command xmlns="…" action="next" node="{node}" sessionid="{session_id}">
    ///     <x xmlns="jabber:x:data" type="submit">
    ///       <field var="{var}"><value>{value}</value></field>
    ///     </x>
    ///   </command>
    /// </iq>
    /// ```
    ///
    /// Returns `(iq_id, element)`.
    pub fn build_continue(
        &mut self,
        to_jid: &str,
        node: &str,
        session_id: &str,
        fields: &[DataField],
    ) -> (String, Element) {
        let id = Uuid::new_v4().to_string();

        let mut x_builder = Element::builder("x", NS_DATA).attr("type", "submit");
        for field in fields {
            let mut field_builder = Element::builder("field", NS_DATA).attr("var", &field.var);
            if let Some(ref v) = field.value {
                let value_el = Element::builder("value", NS_DATA)
                    .append(v.as_str())
                    .build();
                field_builder = field_builder.append(value_el);
            }
            x_builder = x_builder.append(field_builder.build());
        }
        let x = x_builder.build();

        let command = Element::builder("command", NS_ADHOC)
            .attr("action", "next")
            .attr("node", node)
            .attr("sessionid", session_id)
            .append(x)
            .build();

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("id", &id)
            .attr("to", to_jid)
            .append(command)
            .build();

        self.pending.insert(id.clone(), node.to_string());
        (id, iq)
    }

    /// Build an IQ to cancel a command session.
    ///
    /// ```xml
    /// <iq type="set" id="{id}" to="{jid}">
    ///   <command xmlns="…" action="cancel" node="{node}" sessionid="{session_id}"/>
    /// </iq>
    /// ```
    ///
    /// Returns `(iq_id, element)`.
    pub fn build_cancel(
        &mut self,
        to_jid: &str,
        node: &str,
        session_id: &str,
    ) -> (String, Element) {
        let id = Uuid::new_v4().to_string();
        let command = Element::builder("command", NS_ADHOC)
            .attr("action", "cancel")
            .attr("node", node)
            .attr("sessionid", session_id)
            .build();
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("id", &id)
            .attr("to", to_jid)
            .append(command)
            .build();
        self.pending.insert(id.clone(), node.to_string());
        (id, iq)
    }

    /// Parse an incoming command result IQ.
    ///
    /// If the IQ `id` matches a pending request, removes it from `pending`,
    /// parses the `<command>` child (status, fields, notes) and returns
    /// `Some(CommandResponse)`.
    ///
    /// Returns `None` when the element is not a recognised pending result.
    pub fn on_result(&mut self, el: &Element) -> Option<CommandResponse> {
        let iq_type = el.attr("type")?;
        if iq_type != "result" {
            return None;
        }
        let iq_id = el.attr("id")?;
        // Only accept if we have a pending entry for this id.
        self.pending.remove(iq_id)?;

        let command = el
            .children()
            .find(|c| c.name() == "command" && c.ns() == NS_ADHOC)?;

        let node = command.attr("node").unwrap_or("").to_string();
        let session_id = command.attr("sessionid").unwrap_or("").to_string();
        let status = match command.attr("status").unwrap_or("executing") {
            "completed" => CommandStatus::Completed,
            "canceled" => CommandStatus::Canceled,
            _ => CommandStatus::Executing,
        };

        // Parse x-data form if present.
        let fields = command
            .children()
            .find(|c| c.name() == "x" && c.ns() == NS_DATA)
            .map(parse_data_form)
            .unwrap_or_default();

        // Parse <note> children.
        let notes: Vec<String> = command
            .children()
            .filter(|c| c.name() == "note" && c.ns() == NS_ADHOC)
            .map(tokio_xmpp::minidom::Element::text)
            .collect();

        Some(CommandResponse {
            session_id,
            node,
            status,
            fields,
            notes,
        })
    }
}

impl Default for AdhocManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse all `<field>` children of an x-data `<x>` element.
fn parse_data_form(x: &Element) -> Vec<DataField> {
    x.children()
        .filter(|c| c.name() == "field")
        .map(|field| {
            let var = field.attr("var").unwrap_or("").to_string();
            let label = field.attr("label").map(str::to_string);
            let field_type = field.attr("type").unwrap_or("text-single").to_string();

            let value = field
                .children()
                .find(|c| c.name() == "value")
                .map(tokio_xmpp::minidom::Element::text);

            let options: Vec<(String, String)> = field
                .children()
                .filter(|c| c.name() == "option")
                .map(|opt| {
                    let opt_label = opt.attr("label").unwrap_or("").to_string();
                    let opt_value = opt
                        .children()
                        .find(|c| c.name() == "value")
                        .map(tokio_xmpp::minidom::Element::text)
                        .unwrap_or_default();
                    (opt_label, opt_value)
                })
                .collect();

            DataField {
                var,
                label,
                field_type,
                value,
                options,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // 1 -----------------------------------------------------------------------
    #[test]
    fn build_execute_registers_pending() {
        let mut mgr = AdhocManager::new();
        let (id, _el) = mgr.build_execute("admin.example.org", "list-commands");

        assert!(!id.is_empty());
        assert!(mgr.pending.contains_key(&id));
        assert_eq!(mgr.pending[&id], "list-commands");
    }

    // 2 -----------------------------------------------------------------------
    #[test]
    fn build_execute_has_action_execute() {
        let mut mgr = AdhocManager::new();
        let (_id, el) = mgr.build_execute("admin.example.org", "change-user-password");

        assert_eq!(el.attr("type"), Some("set"));
        let cmd = el
            .children()
            .find(|c| c.name() == "command")
            .expect("no command child");
        assert_eq!(cmd.ns(), NS_ADHOC);
        assert_eq!(cmd.attr("action"), Some("execute"));
        assert_eq!(cmd.attr("node"), Some("change-user-password"));
    }

    // 3 -----------------------------------------------------------------------
    #[test]
    fn build_cancel_has_action_cancel() {
        let mut mgr = AdhocManager::new();
        let (_id, el) = mgr.build_cancel("admin.example.org", "change-user-password", "session-42");

        let cmd = el
            .children()
            .find(|c| c.name() == "command")
            .expect("no command child");
        assert_eq!(cmd.attr("action"), Some("cancel"));
        assert_eq!(cmd.attr("sessionid"), Some("session-42"));
    }

    // 4 -----------------------------------------------------------------------
    #[test]
    fn on_result_parses_status_completed() {
        let mut mgr = AdhocManager::new();
        let (id, _) = mgr.build_execute("admin.example.org", "announce");

        let command = Element::builder("command", NS_ADHOC)
            .attr("node", "announce")
            .attr("sessionid", "sess-1")
            .attr("status", "completed")
            .build();
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", &id)
            .append(command)
            .build();

        let resp = mgr.on_result(&iq).expect("expected a response");
        assert_eq!(resp.status, CommandStatus::Completed);
        assert_eq!(resp.node, "announce");
        assert_eq!(resp.session_id, "sess-1");
    }

    // 5 -----------------------------------------------------------------------
    #[test]
    fn on_result_parses_fields() {
        let mut mgr = AdhocManager::new();
        let (id, _) = mgr.build_execute("admin.example.org", "get-user-info");

        let value_el = Element::builder("value", NS_DATA).append("alice").build();
        let option_value = Element::builder("value", NS_DATA).append("a").build();
        let option_el = Element::builder("option", NS_DATA)
            .attr("label", "Option A")
            .append(option_value)
            .build();
        let field_el = Element::builder("field", NS_DATA)
            .attr("var", "username")
            .attr("type", "text-single")
            .attr("label", "Username")
            .append(value_el)
            .append(option_el)
            .build();
        let x = Element::builder("x", NS_DATA)
            .attr("type", "form")
            .append(field_el)
            .build();
        let command = Element::builder("command", NS_ADHOC)
            .attr("node", "get-user-info")
            .attr("sessionid", "sess-2")
            .attr("status", "executing")
            .append(x)
            .build();
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", &id)
            .append(command)
            .build();

        let resp = mgr.on_result(&iq).expect("expected a response");
        assert_eq!(resp.status, CommandStatus::Executing);
        assert_eq!(resp.fields.len(), 1);

        let field = &resp.fields[0];
        assert_eq!(field.var, "username");
        assert_eq!(field.label, Some("Username".to_string()));
        assert_eq!(field.field_type, "text-single");
        assert_eq!(field.value, Some("alice".to_string()));
        assert_eq!(field.options.len(), 1);
        assert_eq!(field.options[0], ("Option A".to_string(), "a".to_string()));
    }

    // 6 -----------------------------------------------------------------------
    #[test]
    fn on_result_clears_pending() {
        let mut mgr = AdhocManager::new();
        let (id, _) = mgr.build_execute("admin.example.org", "restart");

        let command = Element::builder("command", NS_ADHOC)
            .attr("node", "restart")
            .attr("sessionid", "sess-3")
            .attr("status", "completed")
            .build();
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", &id)
            .append(command)
            .build();

        mgr.on_result(&iq);
        assert!(!mgr.pending.contains_key(&id));
    }
}
