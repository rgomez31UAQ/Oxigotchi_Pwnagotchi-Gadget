//! Bull-themed status messages keyed by face name.
//!
//! Ported from Python angryoxide.py v2.3.0 BULL_MESSAGES dict.
//! Messages cycle slowly (3 epochs per message) for readability on e-ink.

use std::collections::HashMap;
use std::sync::LazyLock;

static DEFAULT_MESSAGES: &[&str] = &["AO scanning..."];

pub static BULL_MESSAGES: LazyLock<HashMap<&'static str, Vec<&'static str>>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("bored", vec![
        "This is bull...shit wifi coverage",
        "Herd any good networks?",
        "I'm not amoosed",
        "Udderly bored",
        "Chewing cud, waiting for packets",
        "Standing in the pasture... alone",
        "Not even a stale beacon out here",
    ]);
    m.insert("happy", vec![
        "That was legendairy!",
        "Horns up! Got one!",
        "No bull -- that was clean!",
        "Moo-velous capture!",
        "The herd eats tonight!",
        "Grade-A handshake!",
        "Cream of the crop catch!",
    ]);
    m.insert("excited", vec![
        "Holy cow! PMKID!",
        "This is un-bull-ievable!",
        "Steak dinner tonight!",
        "The bull charges! Got a big one!",
        "Stampede of packets!",
        "Rare catch! Well done!",
        "That's prime beef right there!",
    ]);
    m.insert("lonely", vec![
        "Where's the herd?",
        "Feeling pasture prime",
        "Grazing alone...",
        "This bull needs company",
        "Mooing into the void...",
        "Echo... echo... moo...",
        "One lonely bull in a big field",
    ]);
    m.insert("angry", vec![
        "Don't have a cow, man",
        "Bull in a china shop mode",
        "Seeing red!",
        "These horns aren't just for show",
        "Snorting and stamping!",
        "Who moved my hay bale?!",
        "This bull has HAD it!",
    ]);
    m.insert("cool", vec![
        "Too cool for the barn",
        "Calf-way to greatness",
        "Smooth as butter",
        "Ice cold horns",
        "The cool bull rides at night",
        "No sweat, just hooves",
        "Chillin' like a villain bull",
    ]);
    m.insert("sleep", vec![
        "Counting sheep... no wait, APs",
        "Moo...zzz",
        "Hay there, I'm sleeping",
        "Dreaming of green pastures",
        "Out to pasture for the night",
        "Bull nap in progress",
        "Do not disturb the bull",
    ]);
    m.insert("awake", vec![
        "Rise and grind!",
        "The bull awakens",
        "Time to stampede!",
        "Morning dew on these horns",
        "Fresh hooves, fresh start",
        "Stretching the horns out",
        "Another day, another hay bale",
    ]);
    m.insert("motivated", vec![
        "Let's moooove!",
        "No horns barred!",
        "Charge!",
        "Full steam ahead!",
        "This bull means business!",
        "Hoofing it to victory!",
        "Born to run, built to capture!",
    ]);
    m.insert("smart", vec![
        "Big brain bovine",
        "The sage of the pasture",
        "Calculated like a ruminating mind",
        "400 IQ bull move",
        "Outsmarting the whole barn",
        "Thinking with both horns",
    ]);
    m.insert("grateful", vec![
        "You're the cream of the crop!",
        "Thanks for the feed!",
        "Best rancher ever!",
        "Moo-ch appreciated!",
        "This bull loves you!",
        "You keep me well-fed!",
    ]);
    m.insert("friend", vec![
        "Herd mentality activated!",
        "A fellow bull!",
        "Two horns are better than one!",
        "The herd grows!",
        "Moo-tual respect!",
        "Found my bovine buddy!",
    ]);
    m.insert("upload", vec![
        "Sending to the cloud... pasture",
        "Uploading the goods",
        "Beaming hay to the barn",
        "Data mooo-ving upstream",
        "Sharing the spoils with the herd",
    ]);
    m.insert("debug", vec![
        "Checking under the hood...",
        "Running bull diagnostics",
        "Inspecting the hooves",
        "Veterinary self-check",
        "Calibrating the horns",
        "Bull system check in progress",
    ]);
    m.insert("demotivated", vec![
        "This pasture is dried up",
        "Even the grass is gone",
        "Moo...ving might help",
        "The bull spirit is fading",
        "Running on empty hay",
        "Need greener pastures",
    ]);
    m.insert("sad", vec![
        "A bull with no field...",
        "Milk me, I'm sad",
        "These horns feel heavy",
        "Rainy day at the ranch",
        "Missing the golden pastures",
        "Even the barn feels empty",
    ]);
    m.insert("intense", vec![
        "Locked on target!",
        "The bull sees everything",
        "Horns down, eyes forward",
        "Full intensity stampede!",
        "No AP escapes this bull",
    ]);
    m.insert("look_r", vec![
        "Something over there...",
        "The bull glances right",
        "Ears perked, eyes right",
        "What's rustling in that bush?",
        "Spotted something interesting",
    ]);
    m.insert("look_l", vec![
        "Movement to the left!",
        "The bull peers leftward",
        "Could be a new network...",
        "Turning the horns that way",
        "Left field action!",
    ]);
    m
});

/// Get messages for a face name. Falls back to default if face has no messages.
pub fn messages_for_face(face: &str) -> &[&'static str] {
    BULL_MESSAGES
        .get(face)
        .map(|v| v.as_slice())
        .unwrap_or(DEFAULT_MESSAGES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_message_faces_have_entries() {
        let faces = ["bored", "happy", "excited", "lonely", "angry", "cool",
                      "sleep", "awake", "motivated", "smart", "grateful",
                      "friend", "upload", "debug", "demotivated", "sad",
                      "intense"];
        for face in &faces {
            let msgs = messages_for_face(face);
            assert!(!msgs.is_empty(), "face '{}' should have messages", face);
        }
    }

    #[test]
    fn test_total_message_count() {
        let total: usize = BULL_MESSAGES.values().map(|v| v.len()).sum();
        assert!(total >= 100, "should have at least 100 messages, got {}", total);
    }

    #[test]
    fn test_look_faces_have_messages() {
        assert!(!messages_for_face("look_r").is_empty());
        assert!(!messages_for_face("look_l").is_empty());
    }

    #[test]
    fn test_unknown_face_returns_default() {
        let msgs = messages_for_face("nonexistent");
        assert_eq!(msgs, DEFAULT_MESSAGES);
    }
}
