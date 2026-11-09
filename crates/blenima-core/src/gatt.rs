//! What GATT tells about itself: characteristic properties, the Presentation
//! Format descriptor, and errors in words.

use serde::Serialize;

/// Characteristic properties, the bits of the declaration, in words.
pub fn property_names(bits: u8) -> Vec<&'static str> {
    const NAMES: [&str; 8] = [
        "broadcast",
        "read",
        "write without response",
        "write",
        "notify",
        "indicate",
        "signed write",
        "extended properties",
    ];
    NAMES
        .iter()
        .enumerate()
        .filter(|(bit, _)| bits & (1 << bit) != 0)
        .map(|(_, name)| *name)
        .collect()
}

/// The Characteristic Presentation Format descriptor, 0x2904: how to read a value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PresentationFormat {
    /// The value's format, as the UI names it: "u16", "i32", "f32", "utf8"...
    pub format: &'static str,
    /// The value is multiplied by 10 to this power.
    pub exponent: i8,
    /// The unit's assigned number, such as 0x272F for degrees Celsius.
    pub unit: u16,
    /// The unit as a symbol, where it is a common one: "°C", "V", "%".
    pub unit_symbol: Option<&'static str>,
}

/// Reads a Presentation Format descriptor's 7 bytes.
pub fn presentation_format(bytes: &[u8]) -> Option<PresentationFormat> {
    if bytes.len() < 4 {
        return None;
    }
    let format = match bytes[0] {
        0x01 => "bool",
        0x04 => "u8",
        0x06 => "u16",
        0x07 => "u24",
        0x08 => "u32",
        0x0A => "u64",
        0x0C => "i8",
        0x0E => "i16",
        0x0F => "i24",
        0x10 => "i32",
        0x12 => "i64",
        0x14 => "f32",
        0x15 => "f64",
        0x16 => "sfloat",
        0x17 => "float",
        0x19 => "utf8",
        _ => "hex",
    };
    let unit = u16::from_le_bytes([bytes[2], bytes[3]]);
    Some(PresentationFormat {
        format,
        exponent: bytes[1] as i8,
        unit,
        unit_symbol: unit_symbol(unit),
    })
}

/// The symbols of common units, by their assigned numbers.
pub fn unit_symbol(unit: u16) -> Option<&'static str> {
    Some(match unit {
        0x2700 => "",
        0x2701 => "m",
        0x2702 => "kg",
        0x2703 => "s",
        0x2704 => "A",
        0x2705 => "K",
        0x2722 => "Hz",
        0x2723 => "N",
        0x2724 => "Pa",
        0x2725 => "J",
        0x2726 => "W",
        0x2727 => "C",
        0x2728 => "V",
        0x272F => "°C",
        0x27AD => "%",
        _ => return None,
    })
}

/// An ATT error code in words, from the Core spec, Vol 3, Part F, 3.4.1.1.
pub fn att_error(code: u8) -> &'static str {
    match code {
        0x01 => "Invalid handle",
        0x02 => "Read not permitted",
        0x03 => "Write not permitted",
        0x04 => "Invalid PDU",
        0x05 => "Insufficient authentication: the device wants to be paired",
        0x06 => "Request not supported",
        0x07 => "Invalid offset",
        0x08 => "Insufficient authorization",
        0x09 => "Prepare queue full",
        0x0A => "Attribute not found",
        0x0B => "Attribute not long: it cannot be read in parts",
        0x0C => "Encryption key size too short",
        0x0D => "Invalid attribute value length",
        0x0E => "Unlikely error",
        0x0F => "Insufficient encryption: the device wants an encrypted link",
        0x10 => "Unsupported group type",
        0x11 => "Insufficient resources",
        0x12 => "Database out of sync",
        0x13 => "Value not allowed",
        0x80..=0x9F => "Application error, from the device's own firmware",
        0xFC => "Write request rejected",
        0xFD => "Client characteristic configuration descriptor improperly configured",
        0xFE => "Procedure already in progress",
        0xFF => "Out of range",
        _ => "Reserved error code",
    }
}

/// An error from the OS put into words a person can act on, with the OS's own
/// words after it.
pub fn explain(error: &str) -> String {
    let lower = error.to_lowercase();
    // An ATT error code, as BlueZ and some others give it: "ATT error: 0x05".
    if let Some(code) = att_code(&lower) {
        return format!("ATT error 0x{code:02X}: {}. ({error})", att_error(code));
    }
    let meaning = if lower.contains("unreachable") {
        "The device did not answer: it is out of range, off, or connected elsewhere"
    } else if lower.contains("accessdenied") || lower.contains("access denied") {
        "The device or the OS refused access: it may need to be paired first"
    } else if lower.contains("protocolerror") || lower.contains("protocol error") {
        "The device refused it with an ATT error"
    } else if lower.contains("not permitted") {
        "The device does not allow that on this characteristic"
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "The device did not answer in time"
    } else if lower.contains("notconnected") || lower.contains("not connected") {
        "The device is not connected"
    } else if lower.contains("notsupported") || lower.contains("not supported") {
        "The OS does not support that"
    } else {
        return error.to_string();
    };
    format!("{meaning}. ({error})")
}

fn att_code(lower: &str) -> Option<u8> {
    let at = lower.find("att error")?;
    let rest = &lower[at..];
    let hex = rest.find("0x")?;
    u8::from_str_radix(rest.get(hex + 2..hex + 4)?, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_are_named() {
        assert_eq!(property_names(0x12), ["read", "notify"]);
        assert_eq!(property_names(0x0C), ["write without response", "write"]);
        assert!(property_names(0).is_empty());
    }

    #[test]
    fn a_presentation_format_is_read() {
        // sint16, exponent -2, degrees Celsius, namespace 1, description 0.
        let format = presentation_format(&[0x0E, 0xFE, 0x2F, 0x27, 0x01, 0x00, 0x00]).unwrap();
        assert_eq!(format.format, "i16");
        assert_eq!(format.exponent, -2);
        assert_eq!(format.unit, 0x272F);
        assert_eq!(format.unit_symbol, Some("°C"));
        assert_eq!(presentation_format(&[0x04]), None);
    }

    #[test]
    fn errors_are_put_into_words() {
        assert!(
            explain("Windows UWP threw error on read: Unreachable")
                .starts_with("The device did not answer")
        );
        assert!(explain("ATT error: 0x05").contains("wants to be paired"));
        assert_eq!(explain("something else"), "something else");
        assert_eq!(
            att_error(0x0F),
            "Insufficient encryption: the device wants an encrypted link"
        );
    }
}
