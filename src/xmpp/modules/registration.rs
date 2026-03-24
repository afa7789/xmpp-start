use tokio_xmpp::minidom::Element;

/// J9: XEP-0077 In-Band Registration
pub const NS_REGISTER: &str = "jabber:iq:register";

pub struct RegistrationManager;

impl RegistrationManager {
    pub fn new() -> Self {
        Self
    }

    /// Build a discovery IQ to request registration fields from the server.
    /// <iq type='get' id='reg1'><query xmlns='jabber:iq:register'/></iq>
    pub fn build_get_form(id: &str) -> Element {
        Element::builder("iq", "jabber:client")
            .attr("type", "get")
            .attr("id", id)
            .append(Element::builder("query", NS_REGISTER).build())
            .build()
    }

    /// Build a simple registration submission IQ.
    /// <iq type='set' id='reg2'><query xmlns='jabber:iq:register'><username>...</username><password>...</password></query></iq>
    pub fn build_registration_submit(
        id: &str,
        username: &str,
        password: &str,
        email: Option<&str>,
    ) -> Element {
        let mut query = Element::builder("query", NS_REGISTER);
        query = query.append(Element::builder("username", NS_REGISTER).append(username).build());
        query = query.append(Element::builder("password", NS_REGISTER).append(password).build());
        if let Some(email) = email {
            query = query.append(Element::builder("email", NS_REGISTER).append(email).build());
        }

        Element::builder("iq", "jabber:client")
            .attr("type", "set")
            .attr("id", id)
            .append(query.build())
            .build()
    }

    /// Build a submission IQ using a filled Data Form (XEP-0004).
    /// <iq type='set' id='reg2'><query xmlns='jabber:iq:register'><x xmlns='jabber:x:data' type='submit'>...</x></query></iq>
    pub fn build_registration_form_submit(id: &str, form: Element) -> Element {
        let query = Element::builder("query", NS_REGISTER)
            .append(form)
            .build();

        Element::builder("iq", "jabber:client")
            .attr("type", "set")
            .attr("id", id)
            .append(query)
            .build()
    }

    /// Parse the registration fields/form from an IQ result.
    /// Returns the raw <query> element if it's a registration challenge.
    pub fn parse_registration_query(el: &Element) -> Option<Element> {
        if el.name() == "iq" && el.attr("type") == Some("result") {
            return el.get_child("query", NS_REGISTER).cloned();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_get_form() {
        let el = RegistrationManager::build_get_form("reg1");
        assert_eq!(el.name(), "iq");
        assert_eq!(el.attr("type"), Some("get"));
        let query = el.get_child("query", NS_REGISTER).unwrap();
        assert_eq!(query.name(), "query");
    }

    #[test]
    fn test_build_registration_submit() {
        let el = RegistrationManager::build_registration_submit("reg2", "alice", "secret", Some("alice@example.com"));
        let query = el.get_child("query", NS_REGISTER).unwrap();
        assert_eq!(query.get_child("username", NS_REGISTER).unwrap().text(), "alice");
        assert_eq!(query.get_child("password", NS_REGISTER).unwrap().text(), "secret");
        assert_eq!(query.get_child("email", NS_REGISTER).unwrap().text(), "alice@example.com");
    }
}
