//! Two-part bull jokes keyed by face name.
//!
//! Ported from Python angryoxide.py v2.3.0 BULL_JOKES dict.
//! Question shows for 2 epochs, punchline for 3 epochs (5 total ~2.5 min).

use std::collections::HashMap;
use std::sync::LazyLock;

/// (question, punchline) tuple for a two-part bull joke.
pub type Joke = (&'static str, &'static str);

pub static BULL_JOKES: LazyLock<HashMap<&'static str, Vec<Joke>>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("bored", vec![
        ("Why did the bull become a musician?", "...He had great horns!"),
        ("What's a bull's favorite subject?", "...Bull-gebra!"),
        ("Why don't bulls use smartphones?", "...Too many bull-etins!"),
        ("What do bulls do on weekends?", "...Hit the moo-vies!"),
        ("Why did the bull stare at the gate?", "...He hated being fenced in!"),
        ("What do bulls order at breakfast?", "...Corn flakes and confidence!"),
        ("Why did the bull start gardening?", "...Wanted a better pasture!"),
        ("Why was the bull bad at hide-and-seek?", "...Always stood out in the herd!"),
    ]);
    m.insert("happy", vec![
        ("Why was the bull so calm?", "...No beef today."),
        ("What do you call a rich bull?", "...A stock broker!"),
        ("Why was the bull so confident?", "...Outstanding in his field!"),
        ("Why did the bull avoid arguments?", "...Didn't want any beef!"),
        ("Why was the bull always invited?", "...He brought the energy!"),
    ]);
    m.insert("cool", vec![
        ("What do you call a bull with style?", "...Fashionable beef!"),
        ("Why did the bull get sunglasses?", "...Too much spotlight in the arena!"),
        ("What do you call a polite bull?", "...Well-horned!"),
        ("What's a bull's dream car?", "...A Lamborghini!"),
        ("What do you call a bull detective?", "...Sherlock Horns!"),
    ]);
    m.insert("excited", vec![
        ("Why did the bull get promoted?", "...He really took charge!"),
        ("What's a bull's favorite workout?", "...Charge-io!"),
        ("What's a bull's favorite party move?", "...Charging in late!"),
        ("Why did the bull fail the test?", "...Kept charging ahead!"),
        ("Why did the bull become a chef?", "...He loved grilling!"),
    ]);
    m.insert("angry", vec![
        ("What do you call a bull in a china shop?", "...A demolition expert!"),
        ("Why was the bull kicked out of the library?", "...Too much snorting!"),
        ("What do you call a bull with no manners?", "...Rude beef!"),
        ("Why did the bull get detention?", "...Too much bull in class!"),
        ("What do you call a bull who loves gossip?", "...A moo-s spreader!"),
    ]);
    m.insert("sad", vec![
        ("What's a bull's least favorite weather?", "...A cowld front!"),
        ("Why was the young bull bad at poker?", "...Too easy to read his tells!"),
        ("Why did the bull bring a suitcase?", "...Ready to hoof it!"),
        ("Why did the bull get a map?", "...He kept losing his herd!"),
    ]);
    m.insert("lonely", vec![
        ("What's a bull's favorite app?", "...Anything with more followers!"),
        ("Why did the bull cross the road?", "...To prove he wasn't chicken!"),
        ("What do you call a bull that can sing?", "...A moo-sician's rival!"),
    ]);
    m.insert("sleep", vec![
        ("What do you call a sleeping bull?", "...A bulldozer!"),
        ("Why did the bull sit down?", "...Pasture bedtime!"),
    ]);
    m.insert("motivated", vec![
        ("Why did the bull wear a tie?", "...Big meeting in the pasture!"),
        ("Why did the bull join the gym?", "...To get beefier!"),
        ("Why did the bull open a gym?", "...To help others get shredded beef!"),
        ("What's a bull's favorite game?", "...Truth or dairy!"),
    ]);
    m.insert("smart", vec![
        ("What do you call a bull that paints?", "...Pablo Picowso!"),
        ("Why did the bull go to school?", "...To improve his cow-culus!"),
        ("What's a bull's favorite instrument?", "...The horn section!"),
        ("What's a bull's favorite snack?", "...Chips and dip... mostly dip!"),
    ]);
    m.insert("debug", vec![
        ("Why did the bull start a podcast?", "...Strong opinions and louder breathing!"),
        ("What's a bull's favorite job?", "...Anything in stock management!"),
        ("What do you call a tiny bull?", "...A bulldot!"),
    ]);
    m
});

/// Get jokes for a face name. Falls back to "bored" if face has no jokes.
pub fn jokes_for_face(face: &str) -> &[Joke] {
    BULL_JOKES
        .get(face)
        .map(|v| v.as_slice())
        .unwrap_or_else(|| BULL_JOKES.get("bored").map(|v| v.as_slice()).unwrap_or(&[]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_joke_faces_have_entries() {
        let faces = ["bored", "happy", "cool", "excited", "angry", "sad",
                      "lonely", "sleep", "motivated", "smart", "debug"];
        for face in &faces {
            let jokes = jokes_for_face(face);
            assert!(!jokes.is_empty(), "face '{}' should have jokes", face);
        }
    }

    #[test]
    fn test_each_joke_has_question_and_punchline() {
        for (face, jokes) in BULL_JOKES.iter() {
            for (q, p) in jokes {
                assert!(!q.is_empty(), "joke question empty for face {}", face);
                assert!(p.starts_with("..."), "punchline should start with '...' for face {}: {}", face, p);
            }
        }
    }

    #[test]
    fn test_total_joke_count() {
        let total: usize = BULL_JOKES.values().map(|v| v.len()).sum();
        assert_eq!(total, 48, "expected exactly 48 jokes, got {}", total);
    }

    #[test]
    fn test_unknown_face_returns_bored() {
        let jokes = jokes_for_face("nonexistent");
        assert!(!jokes.is_empty(), "unknown face should fall back to bored jokes");
    }
}
