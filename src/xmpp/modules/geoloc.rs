// Task L3 — XEP-0080 User Location (GeoLoc)
// XEP reference: https://xmpp.org/extensions/xep-0080.html
//
// Pure stanza builder/parser — no I/O, no async.
// Builds PEP publish stanzas for location sharing and parses incoming ones.

use tokio_xmpp::minidom::Element;
use uuid::Uuid;

use super::{find_child_recursive, NS_CLIENT, NS_PUBSUB};

const NS_GEOLOC: &str = "http://jabber.org/protocol/geoloc";

// ---------------------------------------------------------------------------
// Domain type
// ---------------------------------------------------------------------------

/// A geographic location per XEP-0080.
#[derive(Debug, Clone, PartialEq)]
pub struct GeoLocation {
    pub lat: f64,
    pub lon: f64,
    pub accuracy: Option<f64>,
    pub description: Option<String>,
    /// ISO 8601 timestamp string, e.g. "2024-01-15T12:00:00Z"
    pub timestamp: Option<String>,
}

// ---------------------------------------------------------------------------
// Stanza builders
// ---------------------------------------------------------------------------

/// Build a PEP publish IQ for the user's current location.
///
/// ```xml
/// <iq type="set" id="{uuid}">
///   <pubsub xmlns="http://jabber.org/protocol/pubsub">
///     <publish node="http://jabber.org/protocol/geoloc">
///       <item id="current">
///         <geoloc xmlns="http://jabber.org/protocol/geoloc">
///           <lat>51.5</lat>
///           <lon>-0.12</lon>
///           <!-- optional children -->
///         </geoloc>
///       </item>
///     </publish>
///   </pubsub>
/// </iq>
/// ```
pub fn build_geoloc_publish(loc: &GeoLocation) -> Element {
    let id = Uuid::new_v4().to_string();

    let lat_el = Element::builder("lat", NS_GEOLOC)
        .append(loc.lat.to_string().as_str())
        .build();
    let lon_el = Element::builder("lon", NS_GEOLOC)
        .append(loc.lon.to_string().as_str())
        .build();

    let mut geoloc_builder = Element::builder("geoloc", NS_GEOLOC)
        .append(lat_el)
        .append(lon_el);

    if let Some(acc) = loc.accuracy {
        let acc_el = Element::builder("accuracy", NS_GEOLOC)
            .append(acc.to_string().as_str())
            .build();
        geoloc_builder = geoloc_builder.append(acc_el);
    }

    if let Some(ref desc) = loc.description {
        let desc_el = Element::builder("description", NS_GEOLOC)
            .append(desc.as_str())
            .build();
        geoloc_builder = geoloc_builder.append(desc_el);
    }

    if let Some(ref ts) = loc.timestamp {
        let ts_el = Element::builder("timestamp", NS_GEOLOC)
            .append(ts.as_str())
            .build();
        geoloc_builder = geoloc_builder.append(ts_el);
    }

    let item = Element::builder("item", NS_PUBSUB)
        .attr("id", "current")
        .append(geoloc_builder.build())
        .build();

    let publish = Element::builder("publish", NS_PUBSUB)
        .attr("node", NS_GEOLOC)
        .append(item)
        .build();

    let pubsub = Element::builder("pubsub", NS_PUBSUB)
        .append(publish)
        .build();

    Element::builder("iq", NS_CLIENT)
        .attr("type", "set")
        .attr("id", &id)
        .append(pubsub)
        .build()
}

/// Parse a `<geoloc>` element (or a message/item containing one) into a
/// `GeoLocation`. Returns `None` if the required `<lat>` or `<lon>` fields
/// are missing or cannot be parsed.
pub fn parse_geoloc(element: &Element) -> Option<GeoLocation> {
    // Accept either a bare <geoloc> element or any wrapper — walk to find it.
    let geoloc = find_geoloc(element)?;

    let lat: f64 = geoloc
        .children()
        .find(|c| c.name() == "lat" && c.ns() == NS_GEOLOC)
        .and_then(|c| c.text().parse().ok())?;

    let lon: f64 = geoloc
        .children()
        .find(|c| c.name() == "lon" && c.ns() == NS_GEOLOC)
        .and_then(|c| c.text().parse().ok())?;

    let accuracy = geoloc
        .children()
        .find(|c| c.name() == "accuracy" && c.ns() == NS_GEOLOC)
        .and_then(|c| c.text().parse().ok());

    let description = geoloc
        .children()
        .find(|c| c.name() == "description" && c.ns() == NS_GEOLOC)
        .map(|c| c.text().to_string())
        .filter(|s| !s.is_empty());

    let timestamp = geoloc
        .children()
        .find(|c| c.name() == "timestamp" && c.ns() == NS_GEOLOC)
        .map(|c| c.text().to_string())
        .filter(|s| !s.is_empty());

    Some(GeoLocation {
        lat,
        lon,
        accuracy,
        description,
        timestamp,
    })
}

/// Recursively search `el` (depth-first) for a `<geoloc>` element.
fn find_geoloc(el: &Element) -> Option<&Element> {
    find_child_recursive(el, "geoloc", NS_GEOLOC)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_geoloc_element(lat: &str, lon: &str) -> Element {
        let lat_el = Element::builder("lat", NS_GEOLOC).append(lat).build();
        let lon_el = Element::builder("lon", NS_GEOLOC).append(lon).build();
        Element::builder("geoloc", NS_GEOLOC)
            .append(lat_el)
            .append(lon_el)
            .build()
    }

    #[test]
    fn build_publish_has_correct_structure() {
        let loc = GeoLocation {
            lat: 51.5,
            lon: -0.12,
            accuracy: None,
            description: None,
            timestamp: None,
        };
        let el = build_geoloc_publish(&loc);

        assert_eq!(el.name(), "iq");
        assert_eq!(el.attr("type"), Some("set"));

        let pubsub = el
            .children()
            .find(|c| c.name() == "pubsub")
            .expect("no pubsub");
        let publish = pubsub
            .children()
            .find(|c| c.name() == "publish")
            .expect("no publish");
        assert_eq!(publish.attr("node"), Some(NS_GEOLOC));

        let item = publish
            .children()
            .find(|c| c.name() == "item")
            .expect("no item");
        let geoloc = item
            .children()
            .find(|c| c.name() == "geoloc")
            .expect("no geoloc");
        assert_eq!(geoloc.ns(), NS_GEOLOC);
    }

    #[test]
    fn parse_geoloc_roundtrip_basic() {
        let loc = GeoLocation {
            lat: 51.5074,
            lon: -0.1278,
            accuracy: None,
            description: None,
            timestamp: None,
        };
        let el = build_geoloc_publish(&loc);
        let parsed = parse_geoloc(&el).expect("parse failed");

        assert!((parsed.lat - loc.lat).abs() < 1e-6);
        assert!((parsed.lon - loc.lon).abs() < 1e-6);
        assert!(parsed.accuracy.is_none());
        assert!(parsed.description.is_none());
    }

    #[test]
    fn parse_geoloc_roundtrip_full() {
        let loc = GeoLocation {
            lat: 48.8566,
            lon: 2.3522,
            accuracy: Some(10.0),
            description: Some("Paris city centre".to_string()),
            timestamp: Some("2024-01-15T12:00:00Z".to_string()),
        };
        let el = build_geoloc_publish(&loc);
        let parsed = parse_geoloc(&el).expect("parse failed");

        assert!((parsed.lat - loc.lat).abs() < 1e-6);
        assert!((parsed.lon - loc.lon).abs() < 1e-6);
        assert_eq!(parsed.accuracy, Some(10.0));
        assert_eq!(parsed.description.as_deref(), Some("Paris city centre"));
        assert_eq!(
            parsed.timestamp.as_deref(),
            Some("2024-01-15T12:00:00Z")
        );
    }

    #[test]
    fn parse_geoloc_bare_element() {
        let geoloc = make_geoloc_element("40.7128", "-74.0060");
        let parsed = parse_geoloc(&geoloc).expect("parse failed");
        assert!((parsed.lat - 40.7128).abs() < 1e-6);
        assert!((parsed.lon - -74.006).abs() < 1e-6);
    }

    #[test]
    fn parse_geoloc_missing_lat_returns_none() {
        let lon_el = Element::builder("lon", NS_GEOLOC).append("-74.0060").build();
        let geoloc = Element::builder("geoloc", NS_GEOLOC)
            .append(lon_el)
            .build();
        assert!(parse_geoloc(&geoloc).is_none());
    }

    #[test]
    fn parse_geoloc_missing_lon_returns_none() {
        let lat_el = Element::builder("lat", NS_GEOLOC).append("40.7128").build();
        let geoloc = Element::builder("geoloc", NS_GEOLOC)
            .append(lat_el)
            .build();
        assert!(parse_geoloc(&geoloc).is_none());
    }
}
