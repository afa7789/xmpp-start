// S10: XEP-0004 Data Forms renderer
// Reference: https://xmpp.org/extensions/xep-0004.html
//
// Renders <x xmlns="jabber:x:data"> forms into iced UI elements.
// Used for: ad-hoc commands, MUC room configuration, user registration, etc.

use iced::{
    widget::{column, container, row, text, text_input, Column},
    Element, Length,
};

#[derive(Debug, Clone, PartialEq)]
pub struct DataForm {
    pub title: Option<String>,
    pub instructions: Option<String>,
    pub fields: Vec<FormField>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormField {
    pub var: Option<String>,
    pub field_type: FieldType,
    pub label: Option<String>,
    pub value: Option<String>,
    pub required: bool,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    TextSingle,
    TextPrivate,
    TextMulti,
    Boolean,
    ListSingle,
    ListMulti,
    Fixed,
    Hidden,
    JidSingle,
    JidMulti,
}

#[allow(dead_code)]
impl DataForm {
    pub fn from_element(el: &tokio_xmpp::minidom::Element) -> Option<Self> {
        if el.ns() != "jabber:x:data" {
            return None;
        }

        let title = el
            .children()
            .find(|c| c.name() == "title")
            .map(tokio_xmpp::minidom::Element::text);
        let instructions = el
            .children()
            .find(|c| c.name() == "instructions")
            .map(tokio_xmpp::minidom::Element::text);

        let mut fields = Vec::new();
        for field_el in el.children().filter(|c| c.name() == "field") {
            if let Some(field) = FormField::from_element(field_el) {
                fields.push(field);
            }
        }

        Some(DataForm {
            title,
            instructions,
            fields,
        })
    }
}

#[allow(dead_code)]
impl FormField {
    pub fn from_element(el: &tokio_xmpp::minidom::Element) -> Option<Self> {
        let var = el.attr("var").map(String::from);
        let field_type = match el.attr("type") {
            Some("text-single") | None => FieldType::TextSingle,
            Some("text-private") => FieldType::TextPrivate,
            Some("text-multi") => FieldType::TextMulti,
            Some("boolean") => FieldType::Boolean,
            Some("list-single") => FieldType::ListSingle,
            Some("list-multi") => FieldType::ListMulti,
            Some("fixed") => FieldType::Fixed,
            Some("hidden") => FieldType::Hidden,
            Some("jid-single") => FieldType::JidSingle,
            Some("jid-multi") => FieldType::JidMulti,
            _ => FieldType::TextSingle,
        };

        let label = el.attr("label").map(String::from);
        let value = el
            .children()
            .find(|c| c.name() == "value")
            .map(tokio_xmpp::minidom::Element::text);
        let required = el.children().any(|c| c.name() == "required");

        let options: Vec<(String, String)> = el
            .children()
            .filter(|c| c.name() == "option")
            .filter_map(|opt| {
                let value = opt.children().find(|c| c.name() == "value")?;
                let value_text = value.text();
                let label = opt.attr("label").unwrap_or(&value_text);
                Some((label.to_string(), value_text))
            })
            .collect();

        Some(FormField {
            var,
            field_type,
            label,
            value,
            required,
            options,
        })
    }

    pub fn to_element(&self) -> tokio_xmpp::minidom::Element {
        let type_str = match self.field_type {
            FieldType::TextSingle => "text-single",
            FieldType::TextPrivate => "text-private",
            FieldType::TextMulti => "text-multi",
            FieldType::Boolean => "boolean",
            FieldType::ListSingle => "list-single",
            FieldType::ListMulti => "list-multi",
            FieldType::Fixed => "fixed",
            FieldType::Hidden => "hidden",
            FieldType::JidSingle => "jid-single",
            FieldType::JidMulti => "jid-multi",
        };

        let mut el =
            tokio_xmpp::minidom::Element::builder("field", "jabber:x:data").attr("type", type_str);

        if let Some(ref v) = self.var {
            el = el.attr("var", v);
        }
        if let Some(ref l) = self.label {
            el = el.attr("label", l);
        }

        let mut element = el.build();

        if let Some(ref v) = self.value {
            element.append_child(
                tokio_xmpp::minidom::Element::builder("value", "jabber:x:data")
                    .append(v.as_str())
                    .build(),
            );
        }

        if self.required {
            element.append_child(
                tokio_xmpp::minidom::Element::builder("required", "jabber:x:data").build(),
            );
        }

        for (label, value) in &self.options {
            let mut opt = tokio_xmpp::minidom::Element::builder("option", "jabber:x:data")
                .attr("label", label)
                .build();
            opt.append_child(
                tokio_xmpp::minidom::Element::builder("value", "jabber:x:data")
                    .append(value.as_str())
                    .build(),
            );
            element.append_child(opt);
        }

        element
    }
}

/// Render a XEP-0004 form as a read-only iced element.
///
/// Hidden fields are omitted. Text inputs are shown but not interactive.
/// Use `render_form_interactive` when the user needs to fill in the form.
pub fn render_form<M: Clone + 'static>(form: DataForm) -> Element<'static, M> {
    let mut col: Column<M> = column![].spacing(12).padding(16);

    if let Some(title) = form.title.clone() {
        col = col.push(text(title).size(18));
    }

    if let Some(instructions) = form.instructions.clone() {
        col = col.push(text(instructions).size(12));
    }

    for field in form.fields {
        let label = field.label.or(field.var.clone()).unwrap_or_default();

        match field.field_type {
            FieldType::Hidden => {
                // Skip hidden fields
            }
            FieldType::Fixed => {
                if let Some(value) = field.value {
                    col = col.push(text(value).size(13));
                }
            }
            FieldType::Boolean => {
                let checked = field.value.is_some_and(|v| v == "1" || v == "true");
                col = col.push(row![
                    text(label).size(14),
                    text(if checked { "☑" } else { "☐" }).size(16),
                ]);
            }
            FieldType::ListSingle | FieldType::ListMulti => {
                let selected: Vec<String> = field
                    .value
                    .map(|v| v.split(',').map(String::from).collect())
                    .unwrap_or_default();

                let mut field_col: Column<M> = column![text(label).size(14)].spacing(4);
                for (opt_label, opt_value) in field.options {
                    let is_selected = selected.contains(&opt_value);
                    let prefix = if field.field_type == FieldType::ListMulti {
                        if is_selected {
                            "☑"
                        } else {
                            "☐"
                        }
                    } else if is_selected {
                        "◉"
                    } else {
                        "○"
                    };
                    field_col = field_col.push(
                        container(text(format!("  {} {}", prefix, opt_label)).size(16))
                            .padding([2, 0]),
                    );
                }
                col = col.push(field_col);
            }
            FieldType::TextSingle | FieldType::TextPrivate | FieldType::JidSingle => {
                let input_value = field.value.unwrap_or_default();
                col = col.push(row![
                    text(label).size(14).width(Length::Fixed(120.0)),
                    text_input("", &input_value).width(Length::Fill),
                ]);
            }
            FieldType::TextMulti | FieldType::JidMulti => {
                let value = field.value.unwrap_or_default();
                col = col.push(column![
                    text(label).size(14),
                    text_input("", &value).width(Length::Fill),
                ]);
            }
        }
    }

    container(col).width(Length::Fill).into()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_field_element(
        var: &str,
        type_str: &str,
        value: Option<&str>,
    ) -> tokio_xmpp::minidom::Element {
        let mut el = tokio_xmpp::minidom::Element::builder("field", "jabber:x:data")
            .attr("var", var)
            .attr("type", type_str)
            .build();
        if let Some(v) = value {
            el.append_child(
                tokio_xmpp::minidom::Element::builder("value", "jabber:x:data")
                    .append(v)
                    .build(),
            );
        }
        el
    }

    fn make_form_element(
        fields: Vec<tokio_xmpp::minidom::Element>,
    ) -> tokio_xmpp::minidom::Element {
        let mut el = tokio_xmpp::minidom::Element::builder("x", "jabber:x:data")
            .attr("type", "form")
            .build();
        for field in fields {
            el.append_child(field);
        }
        el
    }

    #[test]
    fn form_field_round_trip() {
        let field = FormField {
            var: Some("muc#roomconfig_roomname".into()),
            field_type: FieldType::TextSingle,
            label: Some("Room Name".into()),
            value: Some("my-room".into()),
            required: true,
            options: vec![],
        };

        let el = field.to_element();
        let parsed = FormField::from_element(&el).expect("from_element should parse back");

        assert_eq!(parsed.var, field.var);
        assert_eq!(parsed.field_type, field.field_type);
        assert_eq!(parsed.value, field.value);
        assert!(parsed.required);
    }

    #[test]
    fn data_form_from_element_parses_fields() {
        let field1 = make_field_element(
            "FORM_TYPE",
            "hidden",
            Some("http://jabber.org/protocol/muc#roomconfig"),
        );
        let field2 =
            make_field_element("muc#roomconfig_roomname", "text-single", Some("Test Room"));
        let form_el = make_form_element(vec![field1, field2]);

        let form = DataForm::from_element(&form_el).expect("should parse DataForm");
        assert_eq!(form.fields.len(), 2);
        assert_eq!(form.fields[0].field_type, FieldType::Hidden);
        assert_eq!(form.fields[1].value.as_deref(), Some("Test Room"));
    }

    #[test]
    fn data_form_wrong_namespace_returns_none() {
        let el = tokio_xmpp::minidom::Element::builder("x", "wrong:namespace").build();
        assert!(DataForm::from_element(&el).is_none());
    }

    #[test]
    fn field_type_boolean_round_trip() {
        let field = FormField {
            var: Some("muc#roomconfig_publicroom".into()),
            field_type: FieldType::Boolean,
            label: None,
            value: Some("1".into()),
            required: false,
            options: vec![],
        };
        let el = field.to_element();
        let parsed = FormField::from_element(&el).unwrap();
        assert_eq!(parsed.field_type, FieldType::Boolean);
        assert_eq!(parsed.value.as_deref(), Some("1"));
    }
}

/// Render a XEP-0004 form with interactive text inputs.
///
/// `on_change` is called with `(var, new_value)` when the user edits a field.
/// Field values shown are taken from `field_values` (keyed by `var`); if absent,
/// the form's own default value is used.
pub fn render_form_interactive<M, F>(
    form: DataForm,
    field_values: &std::collections::HashMap<String, String>,
    on_change: F,
) -> Element<'static, M>
where
    M: Clone + 'static,
    F: Fn(String, String) -> M + Clone + 'static,
{
    let mut col: Column<M> = column![].spacing(12).padding(16);

    if let Some(title) = form.title.clone() {
        col = col.push(text(title).size(18));
    }

    if let Some(instructions) = form.instructions.clone() {
        col = col.push(text(instructions).size(12));
    }

    for field in form.fields {
        let label = field
            .label
            .clone()
            .or(field.var.clone())
            .unwrap_or_default();

        match field.field_type {
            FieldType::Hidden => {
                // Skip hidden fields — values are submitted but not shown.
            }
            FieldType::Fixed => {
                if let Some(value) = field.value {
                    col = col.push(text(value).size(13));
                }
            }
            FieldType::Boolean => {
                let checked = field.value.is_some_and(|v| v == "1" || v == "true");
                col = col.push(row![
                    text(label).size(14),
                    text(if checked { "☑" } else { "☐" }).size(16),
                ]);
            }
            FieldType::ListSingle | FieldType::ListMulti => {
                let selected: Vec<String> = field
                    .value
                    .map(|v| v.split(',').map(String::from).collect())
                    .unwrap_or_default();

                let mut field_col: Column<M> = column![text(label).size(14)].spacing(4);
                for (opt_label, opt_value) in field.options {
                    let is_selected = selected.contains(&opt_value);
                    let prefix = if field.field_type == FieldType::ListMulti {
                        if is_selected {
                            "☑"
                        } else {
                            "☐"
                        }
                    } else if is_selected {
                        "◉"
                    } else {
                        "○"
                    };
                    field_col = field_col.push(
                        container(text(format!("  {} {}", prefix, opt_label)).size(16))
                            .padding([2, 0]),
                    );
                }
                col = col.push(field_col);
            }
            FieldType::TextSingle | FieldType::TextPrivate | FieldType::JidSingle => {
                let var = field.var.clone().unwrap_or_default();
                let current_val = field_values
                    .get(&var)
                    .cloned()
                    .or(field.value)
                    .unwrap_or_default();
                let on_change_clone = on_change.clone();
                let var_clone = var.clone();
                let input = text_input(&label, &current_val)
                    .on_input(move |v| on_change_clone(var_clone.clone(), v))
                    .width(Length::Fill);
                col = col.push(row![
                    text(label).size(14).width(Length::Fixed(120.0)),
                    input,
                ]);
            }
            FieldType::TextMulti | FieldType::JidMulti => {
                let var = field.var.clone().unwrap_or_default();
                let current_val = field_values
                    .get(&var)
                    .cloned()
                    .or(field.value)
                    .unwrap_or_default();
                let on_change_clone = on_change.clone();
                let var_clone = var.clone();
                let input = text_input(&label, &current_val)
                    .on_input(move |v| on_change_clone(var_clone.clone(), v))
                    .width(Length::Fill);
                col = col.push(column![text(label).size(14), input]);
            }
        }
    }

    container(col).width(Length::Fill).into()
}
