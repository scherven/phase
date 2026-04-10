use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use engine::database::CardDatabase;
use engine::game::deck_loading::{
    load_deck_into_state, resolve_deck_list, DeckList, PlayerDeckList, PlayerDeckPayload,
};
use engine::types::game_state::{GameState, WaitingFor};
use engine::types::log::{GameLogEntry, LogCategory, LogSegment};
use engine::types::player::PlayerId;
use phase_ai::auto_play::run_ai_actions;
use phase_ai::config::{create_config_for_players, AiDifficulty, Platform};

const MAX_TOTAL_ACTIONS: usize = 10_000;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut verbose = false;
    let mut batch: Option<usize> = None;
    let mut seed: Option<u64> = None;
    let mut difficulty = AiDifficulty::Medium;
    let mut matchup = "red-vs-green".to_string();

    let mut args_iter = args.iter().skip(1).peekable();
    while let Some(arg) = args_iter.next() {
        match arg.as_str() {
            "--verbose" => verbose = true,
            "--batch" => batch = args_iter.next().and_then(|v| v.parse().ok()),
            "--seed" => seed = args_iter.next().and_then(|v| v.parse().ok()),
            "--difficulty" => {
                if let Some(level) = args_iter.next() {
                    difficulty = parse_difficulty(level);
                }
            }
            "--matchup" => {
                if let Some(m) = args_iter.next() {
                    matchup = m.clone();
                }
            }
            "--list-matchups" => {
                list_matchups();
                return;
            }
            _ => {}
        }
    }

    let path = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with("--"))
        .cloned()
        .or_else(|| std::env::var("PHASE_CARDS_PATH").ok())
        .map(PathBuf::from);

    let Some(path) = path else {
        eprintln!("Usage: ai-duel <data-root> [OPTIONS]");
        eprintln!("  Or set PHASE_CARDS_PATH environment variable");
        eprintln!();
        eprintln!("Options:");
        eprintln!("  --verbose          Print every action (full trace)");
        eprintln!("  --batch N          Run N games, print summary only");
        eprintln!("  --seed S           RNG seed (default: time-based)");
        eprintln!("  --difficulty LEVEL VeryEasy|Easy|Medium|Hard|VeryHard (default: Medium)");
        eprintln!("  --matchup NAME     Deck matchup (default: red-vs-green)");
        eprintln!("  --list-matchups    Show available matchups");
        std::process::exit(1);
    };

    let export_path = path.join("card-data.json");
    let db = match CardDatabase::from_export(&export_path) {
        Ok(db) => db,
        Err(e) => {
            eprintln!(
                "Failed to load card database from {}: {e}",
                export_path.display()
            );
            std::process::exit(1);
        }
    };

    let Some((deck_list, p0_label, p1_label)) = build_matchup_decks(&matchup) else {
        eprintln!("Unknown matchup '{matchup}'. Use --list-matchups to see options.");
        std::process::exit(1);
    };
    let payload = resolve_deck_list(&db, &deck_list);

    validate_deck(&payload.player, 60, &p0_label);
    validate_deck(&payload.opponent, 60, &p1_label);

    let base_seed = seed.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    });

    let game_count = batch.unwrap_or(1);
    let is_batch = batch.is_some();

    let mut p0_wins: usize = 0;
    let mut p1_wins: usize = 0;
    let mut draws: usize = 0;
    let mut total_turns: u32 = 0;
    let mut total_duration_ms: u128 = 0;

    for game_idx in 0..game_count {
        let game_seed = base_seed + game_idx as u64;

        if !is_batch {
            eprintln!("AI Duel — seed: {game_seed}, difficulty: {difficulty:?}");
        }

        let start = Instant::now();
        let (winner, turns) = run_game(&payload, game_seed, difficulty, verbose, is_batch);
        let elapsed = start.elapsed().as_millis();

        match winner {
            Some(PlayerId(0)) => p0_wins += 1,
            Some(_) => p1_wins += 1,
            None => draws += 1,
        }
        total_turns += turns;
        total_duration_ms += elapsed;

        if !is_batch {
            match winner {
                Some(PlayerId(0)) => {
                    eprintln!("\nGame over — {p0_label} (P0) wins on turn {turns} ({elapsed}ms)")
                }
                Some(_) => {
                    eprintln!("\nGame over — {p1_label} (P1) wins on turn {turns} ({elapsed}ms)")
                }
                None => eprintln!("\nGame over — draw/aborted on turn {turns} ({elapsed}ms)"),
            }
        }
    }

    if is_batch {
        let n = game_count;
        let avg_turns = total_turns as f64 / n as f64;
        let avg_ms = total_duration_ms as f64 / n as f64;
        eprintln!("\nResults ({n} games, seed: {base_seed}, difficulty: {difficulty:?}, matchup: {matchup}):");
        eprintln!(
            "  P0 ({p0_label}) wins: {p0_wins:>4} ({:.1}%)",
            p0_wins as f64 / n as f64 * 100.0
        );
        eprintln!(
            "  P1 ({p1_label}) wins: {p1_wins:>4} ({:.1}%)",
            p1_wins as f64 / n as f64 * 100.0
        );
        eprintln!(
            "  Draws/aborted:             {draws:>4} ({:.1}%)",
            draws as f64 / n as f64 * 100.0
        );
        eprintln!("  Avg turns: {avg_turns:.1}");
        eprintln!("  Avg duration: {avg_ms:.0}ms");
    }
}

fn run_game(
    payload: &engine::game::deck_loading::DeckPayload,
    seed: u64,
    difficulty: AiDifficulty,
    verbose: bool,
    silent: bool,
) -> (Option<PlayerId>, u32) {
    let mut state = GameState::new_two_player(seed);
    load_deck_into_state(&mut state, payload);
    engine::game::engine::start_game(&mut state);

    let ai_players: HashSet<PlayerId> = [PlayerId(0), PlayerId(1)].into_iter().collect();
    let config = create_config_for_players(difficulty, Platform::Native, 2);
    let ai_configs: HashMap<PlayerId, _> = [(PlayerId(0), config.clone()), (PlayerId(1), config)]
        .into_iter()
        .collect();

    let mut total_actions: usize = 0;
    let mut last_turn: u32 = 0;

    loop {
        let results = run_ai_actions(&mut state, &ai_players, &ai_configs);
        if results.is_empty() {
            if matches!(state.waiting_for, WaitingFor::GameOver { .. }) {
                break;
            }
            eprintln!("Warning: no AI actions and game not over — breaking");
            break;
        }
        total_actions += results.len();

        if !silent {
            for result in &results {
                if verbose {
                    eprintln!("  ACTION: {:?}", result.action);
                }
                for entry in &result.log_entries {
                    // Print turn header when log entries reference a new turn
                    if entry.turn != last_turn {
                        last_turn = entry.turn;
                        eprintln!("=== Turn {last_turn} ===");
                    }
                    if should_show(entry, verbose) {
                        eprintln!("  {}", render_log_entry(entry));
                    }
                }
            }
        }

        if total_actions >= MAX_TOTAL_ACTIONS {
            eprintln!("Safety: hit {MAX_TOTAL_ACTIONS} total actions — aborting game");
            break;
        }
    }

    let winner = match &state.waiting_for {
        WaitingFor::GameOver { winner } => *winner,
        _ => None,
    };
    (winner, state.turn_number)
}

fn should_show(entry: &GameLogEntry, verbose: bool) -> bool {
    if verbose {
        return true;
    }
    matches!(
        entry.category,
        LogCategory::Stack
            | LogCategory::Combat
            | LogCategory::Life
            | LogCategory::Destroy
            | LogCategory::Special
    )
}

fn render_log_entry(entry: &GameLogEntry) -> String {
    entry
        .segments
        .iter()
        .map(|seg| match seg {
            LogSegment::Text(s) => s.clone(),
            LogSegment::CardName { name, .. } => name.clone(),
            LogSegment::PlayerName { name, .. } => name.clone(),
            LogSegment::Number(n) => n.to_string(),
            LogSegment::Mana(s) => s.clone(),
            LogSegment::Zone(z) => format!("{z:?}"),
            LogSegment::Keyword(k) => k.clone(),
        })
        .collect::<Vec<_>>()
        .join("")
}

fn validate_deck(payload: &PlayerDeckPayload, expected: usize, label: &str) {
    let actual: u32 = payload.main_deck.iter().map(|e| e.count).sum();
    if actual as usize != expected {
        eprintln!("WARNING: {label} resolved {actual}/{expected} cards");
    }
}

fn parse_difficulty(s: &str) -> AiDifficulty {
    match s.to_lowercase().as_str() {
        "veryeasy" => AiDifficulty::VeryEasy,
        "easy" => AiDifficulty::Easy,
        "medium" => AiDifficulty::Medium,
        "hard" => AiDifficulty::Hard,
        "veryhard" => AiDifficulty::VeryHard,
        _ => {
            eprintln!("Unknown difficulty '{s}', using Medium");
            AiDifficulty::Medium
        }
    }
}

fn repeat(name: &str, count: usize) -> Vec<String> {
    vec![name.to_string(); count]
}

fn list_matchups() {
    eprintln!("Available matchups:");
    eprintln!();
    eprintln!("  Starter decks (mono-colored, simple):");
    eprintln!("  red-vs-green       Red Aggro vs Green Midrange (default)");
    eprintln!("  blue-vs-green      Blue Control vs Green Midrange");
    eprintln!("  red-vs-blue        Red Aggro vs Blue Control");
    eprintln!("  black-vs-green     Black Midrange vs Green Midrange");
    eprintln!("  white-vs-red       White Weenie vs Red Aggro");
    eprintln!("  black-vs-blue      Black Midrange vs Blue Control");
    eprintln!("  red-mirror         Red Aggro mirror");
    eprintln!("  green-mirror       Green Midrange mirror");
    eprintln!("  blue-mirror        Blue Control mirror");
    eprintln!();
    eprintln!("  Metagame decks (real competitive lists, multicolor):");
    eprintln!("  azorius-vs-prowess   Pioneer Azorius Control vs Mono-Red Prowess");
    eprintln!("  azorius-vs-gruul     Pioneer Azorius Control vs Gruul Prowess");
    eprintln!("  delver-vs-prowess    Legacy Izzet Delver vs Mono-Red Prowess");
    eprintln!("  azorius-vs-green     Pioneer Azorius Control vs Green Midrange");
    eprintln!("  delver-vs-green      Legacy Izzet Delver vs Green Midrange");
    eprintln!("  prowess-vs-green     Mono-Red Prowess vs Green Midrange");
    eprintln!("  prowess-mirror       Mono-Red Prowess mirror");
}

fn build_matchup_decks(matchup: &str) -> Option<(DeckList, String, String)> {
    let (p0, p0_label, p1, p1_label) = match matchup {
        // Starter decks
        "red-vs-green" => (
            deck_red_aggro(),
            "Red Aggro",
            deck_green_midrange(),
            "Green Midrange",
        ),
        "blue-vs-green" => (
            deck_blue_control(),
            "Blue Control",
            deck_green_midrange(),
            "Green Midrange",
        ),
        "red-vs-blue" => (
            deck_red_aggro(),
            "Red Aggro",
            deck_blue_control(),
            "Blue Control",
        ),
        "black-vs-green" => (
            deck_black_midrange(),
            "Black Midrange",
            deck_green_midrange(),
            "Green Midrange",
        ),
        "white-vs-red" => (
            deck_white_weenie(),
            "White Weenie",
            deck_red_aggro(),
            "Red Aggro",
        ),
        "black-vs-blue" => (
            deck_black_midrange(),
            "Black Midrange",
            deck_blue_control(),
            "Blue Control",
        ),
        "red-mirror" => (
            deck_red_aggro(),
            "Red Aggro (P0)",
            deck_red_aggro(),
            "Red Aggro (P1)",
        ),
        "green-mirror" => (
            deck_green_midrange(),
            "Green Mid (P0)",
            deck_green_midrange(),
            "Green Mid (P1)",
        ),
        "blue-mirror" => (
            deck_blue_control(),
            "Blue Ctrl (P0)",
            deck_blue_control(),
            "Blue Ctrl (P1)",
        ),
        // Metagame decks
        "azorius-vs-prowess" => (
            deck_azorius_control(),
            "Azorius Control",
            deck_mono_red_prowess(),
            "Mono-Red Prowess",
        ),
        "azorius-vs-gruul" => (
            deck_azorius_control(),
            "Azorius Control",
            deck_gruul_prowess(),
            "Gruul Prowess",
        ),
        "delver-vs-prowess" => (
            deck_izzet_delver(),
            "Izzet Delver",
            deck_mono_red_prowess(),
            "Mono-Red Prowess",
        ),
        "azorius-vs-green" => (
            deck_azorius_control(),
            "Azorius Control",
            deck_green_midrange(),
            "Green Midrange",
        ),
        "delver-vs-green" => (
            deck_izzet_delver(),
            "Izzet Delver",
            deck_green_midrange(),
            "Green Midrange",
        ),
        "prowess-vs-green" => (
            deck_mono_red_prowess(),
            "Mono-Red Prowess",
            deck_green_midrange(),
            "Green Midrange",
        ),
        "prowess-mirror" => (
            deck_mono_red_prowess(),
            "RDW Prowess (P0)",
            deck_mono_red_prowess(),
            "RDW Prowess (P1)",
        ),
        _ => return None,
    };

    Some((
        DeckList {
            player: PlayerDeckList {
                main_deck: p0,
                sideboard: Vec::new(),
                commander: Vec::new(),
            },
            opponent: PlayerDeckList {
                main_deck: p1,
                sideboard: Vec::new(),
                commander: Vec::new(),
            },
            ai_decks: Vec::new(),
        },
        p0_label.to_string(),
        p1_label.to_string(),
    ))
}

fn deck_red_aggro() -> Vec<String> {
    let mut d = Vec::with_capacity(60);
    d.extend(repeat("Mountain", 20));
    d.extend(repeat("Goblin Guide", 4));
    d.extend(repeat("Monastery Swiftspear", 4));
    d.extend(repeat("Raging Goblin", 4));
    d.extend(repeat("Jackal Pup", 4));
    d.extend(repeat("Mogg Fanatic", 4));
    d.extend(repeat("Lightning Bolt", 4));
    d.extend(repeat("Shock", 4));
    d.extend(repeat("Lava Spike", 4));
    d.extend(repeat("Searing Spear", 4));
    d.extend(repeat("Skullcrack", 4));
    d
}

fn deck_green_midrange() -> Vec<String> {
    let mut d = Vec::with_capacity(60);
    d.extend(repeat("Forest", 22));
    d.extend(repeat("Llanowar Elves", 4));
    d.extend(repeat("Elvish Mystic", 4));
    d.extend(repeat("Grizzly Bears", 4));
    d.extend(repeat("Kalonian Tusker", 4));
    d.extend(repeat("Centaur Courser", 4));
    d.extend(repeat("Leatherback Baloth", 2));
    d.extend(repeat("Giant Growth", 4));
    d.extend(repeat("Rancor", 4));
    d.extend(repeat("Titanic Growth", 4));
    d.extend(repeat("Rabid Bite", 4));
    d
}

fn deck_blue_control() -> Vec<String> {
    // 26 lands, 8 creatures, 26 spells = 60
    let mut d = Vec::with_capacity(60);
    d.extend(repeat("Island", 26));
    d.extend(repeat("Counterspell", 4));
    d.extend(repeat("Mana Leak", 4));
    d.extend(repeat("Essence Scatter", 2));
    d.extend(repeat("Negate", 2));
    d.extend(repeat("Unsummon", 4));
    d.extend(repeat("Divination", 4));
    d.extend(repeat("Opt", 4));
    d.extend(repeat("Think Twice", 2));
    d.extend(repeat("Air Elemental", 4));
    d.extend(repeat("Frost Titan", 2));
    d.extend(repeat("Mulldrifter", 2));
    d
}

fn deck_black_midrange() -> Vec<String> {
    // 24 lands, 20 creatures, 16 spells = 60
    let mut d = Vec::with_capacity(60);
    d.extend(repeat("Swamp", 24));
    d.extend(repeat("Vampire Nighthawk", 4));
    d.extend(repeat("Gifted Aetherborn", 4));
    d.extend(repeat("Hypnotic Specter", 4));
    d.extend(repeat("Gray Merchant of Asphodel", 4));
    d.extend(repeat("Nighthawk Scavenger", 4));
    d.extend(repeat("Doom Blade", 4));
    d.extend(repeat("Go for the Throat", 4));
    d.extend(repeat("Sign in Blood", 4));
    d.extend(repeat("Read the Bones", 2));
    d.extend(repeat("Duress", 2));
    d
}

// --- Metagame decks (from MTGGoldfish feeds, 100% engine coverage) ---

fn deck_azorius_control() -> Vec<String> {
    // Pioneer Azorius Control — draw-go with wraths, planeswalkers, counterspells
    let mut d = Vec::with_capacity(60);
    d.extend(repeat("Floodfarm Verge", 4));
    d.extend(repeat("Hallowed Fountain", 4));
    d.extend(repeat("Deserted Beach", 3));
    d.extend(repeat("Meticulous Archive", 2));
    d.extend(repeat("Restless Anchorage", 2));
    d.extend(repeat("Fountainport", 2));
    d.extend(repeat("Island", 2));
    d.extend(repeat("Plains", 2));
    d.extend(repeat("Eiganjo, Seat of the Empire", 1));
    d.extend(repeat("Field of Ruin", 2));
    d.extend(repeat("Hall of Storm Giants", 1));
    d.extend(repeat("Otawara, Soaring City", 1));
    // Counterspells
    d.extend(repeat("No More Lies", 4));
    d.extend(repeat("Dovin's Veto", 1));
    d.extend(repeat("Change the Equation", 1));
    // Removal
    d.extend(repeat("March of Otherworldly Light", 4));
    d.extend(repeat("Get Lost", 3));
    d.extend(repeat("Supreme Verdict", 2));
    d.extend(repeat("Farewell", 1));
    // Card advantage
    d.extend(repeat("Consult the Star Charts", 4));
    d.extend(repeat("Stock Up", 2));
    d.extend(repeat("Three Steps Ahead", 1));
    // Threats
    d.extend(repeat("Pinnacle Starcage", 3));
    d.extend(repeat("The Wandering Emperor", 3));
    d.extend(repeat("Teferi, Hero of Dominaria", 2));
    d.extend(repeat("Beza, the Bounding Spring", 2));
    d.extend(repeat("Elspeth, Storm Slayer", 1));
    d
}

fn deck_mono_red_prowess() -> Vec<String> {
    // Pioneer Mono-Red Prowess — spell-heavy aggro with prowess creatures
    let mut d = Vec::with_capacity(60);
    d.extend(repeat("Mountain", 14));
    d.extend(repeat("Den of the Bugbear", 2));
    d.extend(repeat("Ramunap Ruins", 3));
    d.extend(repeat("Rockface Village", 2));
    d.extend(repeat("Sokenzan, Crucible of Defiance", 1));
    // Creatures
    d.extend(repeat("Monastery Swiftspear", 4));
    d.extend(repeat("Soul-Scar Mage", 4));
    d.extend(repeat("Emberheart Challenger", 4));
    d.extend(repeat("Screaming Nemesis", 4));
    d.extend(repeat("Sunspine Lynx", 3));
    // Spells
    d.extend(repeat("Burst Lightning", 4));
    d.extend(repeat("Monstrous Rage", 4));
    d.extend(repeat("Reckless Rage", 4));
    d.extend(repeat("Kumano Faces Kakkazan", 4));
    d.extend(repeat("Lightning Strike", 2));
    d.extend(repeat("Abrade", 1));
    d
}

fn deck_gruul_prowess() -> Vec<String> {
    // Pioneer Gruul Prowess — RG spell-based aggro
    let mut d = Vec::with_capacity(60);
    d.extend(repeat("Mountain", 6));
    d.extend(repeat("Stomping Ground", 4));
    d.extend(repeat("Copperline Gorge", 4));
    d.extend(repeat("Thornspire Verge", 4));
    d.extend(repeat("Den of the Bugbear", 1));
    d.extend(repeat("Ramunap Ruins", 1));
    d.extend(repeat("Sokenzan, Crucible of Defiance", 1));
    // Creatures
    d.extend(repeat("Monastery Swiftspear", 4));
    d.extend(repeat("Soul-Scar Mage", 4));
    d.extend(repeat("Emberheart Challenger", 4));
    d.extend(repeat("Questing Druid", 4));
    d.extend(repeat("Cori-Steel Cutter", 4));
    d.extend(repeat("Screaming Nemesis", 2));
    // Spells
    d.extend(repeat("Burst Lightning", 4));
    d.extend(repeat("Kumano Faces Kakkazan", 4));
    d.extend(repeat("Academic Dispute", 4));
    d.extend(repeat("Reckless Rage", 3));
    d.extend(repeat("Monstrous Rage", 2));
    d
}

fn deck_izzet_delver() -> Vec<String> {
    // Legacy Izzet Delver — tempo-control with free counters and efficient threats
    let mut d = Vec::with_capacity(60);
    d.extend(repeat("Volcanic Island", 4));
    d.extend(repeat("Wasteland", 4));
    d.extend(repeat("Scalding Tarn", 2));
    d.extend(repeat("Misty Rainforest", 2));
    d.extend(repeat("Flooded Strand", 3));
    d.extend(repeat("Polluted Delta", 2));
    d.extend(repeat("Island", 1));
    d.extend(repeat("Thundering Falls", 1));
    // Creatures
    d.extend(repeat("Delver of Secrets", 4));
    d.extend(repeat("Dragon's Rage Channeler", 4));
    d.extend(repeat("Cori-Steel Cutter", 3));
    d.extend(repeat("Murktide Regent", 2));
    d.extend(repeat("Brazen Borrower", 1));
    // Cantrips
    d.extend(repeat("Brainstorm", 4));
    d.extend(repeat("Ponder", 4));
    d.extend(repeat("Mishra's Bauble", 4));
    d.extend(repeat("Preordain", 1));
    // Counters
    d.extend(repeat("Force of Will", 4));
    d.extend(repeat("Daze", 4));
    d.extend(repeat("Spell Pierce", 1));
    // Removal
    d.extend(repeat("Lightning Bolt", 4));
    d.extend(repeat("Unholy Heat", 1));
    d
}

fn deck_white_weenie() -> Vec<String> {
    // 20 lands, 24 creatures, 16 spells = 60
    let mut d = Vec::with_capacity(60);
    d.extend(repeat("Plains", 20));
    d.extend(repeat("Savannah Lions", 4));
    d.extend(repeat("Elite Vanguard", 4));
    d.extend(repeat("Soldier of the Pantheon", 4));
    d.extend(repeat("Thalia, Guardian of Thraben", 4));
    d.extend(repeat("Serra Angel", 4));
    d.extend(repeat("Benalish Marshal", 4));
    d.extend(repeat("Swords to Plowshares", 4));
    d.extend(repeat("Raise the Alarm", 4));
    d.extend(repeat("Glorious Anthem", 4));
    d.extend(repeat("Honor of the Pure", 4));
    d
}
