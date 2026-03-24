// S10: XEP-0004 Data Forms renderer
// Reference: https://xmpp.org/extensions/xep-0004.html
//
// Renders <x xmlns="jabber:x:data"> forms into iced UI elements.
// Used for: ad-hoc commands, MUC room configuration, user registration, etc.

use iced::{
    widget::{column, container, row, text, text_input, Column},
    Element, Length,
};

#[derive(Debug, Clone)]
pub struct DataForm {
    pub title: Option<String>,
    pub instructions: Option<String>,
    pub fields: Vec<FormField>,
}

#[derive(Debug, Clone)]
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

impl DataForm {
    pub fn from_element(el: &tokio_xmpp::minidom::Element) -> Option<Self> {
        if el.ns() != "jabber:x:data" {
            return None;
        }

        let title = el
            .children()
            .find(|c| c.name() == "title")
            .map(|c| c.text());
        let instructions = el
            .children()
            .find(|c| c.name() == "instructions")
            .map(|c| c.text());

        let mut fields = Vec::new();
        for field_el in el.children().filter(|c| c.name() == "field") {
            if let Some(field) = FormField::from_element(&field_el) {
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
            .map(|c| c.text());
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
                let checked = field
                    .value
                    .map(|v| v == "1" || v == "true")
                    .unwrap_or(false);
                col = col.push(row![
                    text(label).size(14),
                    text(if checked { "[x]" } else { "[ ]" }).size(14),
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
                    } else {
                        if is_selected {
                            "●"
                        } else {
                            "○"
                        }
                    };
                    field_col = field_col.push(
                        container(text(format!("  {} {}", prefix, opt_label)).size(13))
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
