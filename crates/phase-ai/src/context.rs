use std::sync::Arc;

use engine::game::DeckEntry;
use engine::types::player::PlayerId;

use crate::deck_profile::ArchetypeMultipliers;
use crate::deck_profile::DeckProfile;
use crate::eval::EvalWeightSet;
use crate::session::AiSession;
use crate::strategy_profile::StrategyProfile;
use crate::synergy::SynergyGraph;
use crate::threat_profile::ThreatProfile;

/// Pre-computed deck analysis, built once per game from the deck pool.
/// Threaded through `PlannerServices` into eval, policies, and search.
///
/// When no deck data is available (e.g., tests, non-deck games), use
/// `AiContext::empty()` which provides neutral defaults that produce
/// identical behavior to the pre-context-aware AI.
#[derive(Debug, Clone)]
pub struct AiContext {
    pub deck_profile: DeckProfile,
    pub adjusted_weights: EvalWeightSet,
    pub strategy: StrategyProfile,
    /// Opponent threat profile (None when threat awareness is disabled).
    pub opponent_threat: Option<ThreatProfile>,
    /// Per-game cache shared across all decisions. Holds the synergy graph
    /// (formerly owned directly by `AiContext`) plus per-player features and
    /// plan snapshots.
    pub session: Arc<AiSession>,
    /// The player whose perspective this context represents. Used to look up
    /// per-player session data (synergy, features, plan).
    pub player: PlayerId,
}

static EMPTY_SYNERGY_GRAPH: std::sync::OnceLock<SynergyGraph> = std::sync::OnceLock::new();

impl AiContext {
    /// Analyze a deck list to build the context.
    pub fn analyze(deck: &[DeckEntry], base_weights: &EvalWeightSet) -> Self {
        Self::analyze_with(deck, base_weights, &ArchetypeMultipliers::default())
    }

    /// Analyze a deck list with custom archetype multipliers.
    /// The returned context uses `PlayerId(0)` as the perspective; callers
    /// that know the AI's player ID should set `context.player` afterward.
    pub fn analyze_with(
        deck: &[DeckEntry],
        base_weights: &EvalWeightSet,
        multipliers: &ArchetypeMultipliers,
    ) -> Self {
        let player = PlayerId(0);
        let deck_profile = DeckProfile::analyze(deck);
        let adjusted_weights = EvalWeightSet {
            early: deck_profile.adjust_weights_with(multipliers, &base_weights.early),
            mid: deck_profile.adjust_weights_with(multipliers, &base_weights.mid),
            late: deck_profile.adjust_weights_with(multipliers, &base_weights.late),
        };
        let strategy = StrategyProfile::for_profile(&deck_profile);
        let session = Arc::new(AiSession::from_single_deck(player, deck));
        Self {
            deck_profile,
            adjusted_weights,
            strategy,
            opponent_threat: None,
            session,
            player,
        }
    }

    /// Neutral context for when no deck data is available.
    /// Strategic dimensions contribute 0.0, weights are unchanged from base.
    pub fn empty(base_weights: &EvalWeightSet) -> Self {
        Self {
            deck_profile: DeckProfile::default(),
            adjusted_weights: base_weights.clone(),
            strategy: StrategyProfile::default(),
            opponent_threat: None,
            session: Arc::new(AiSession::empty()),
            player: PlayerId(0),
        }
    }

    /// Return the synergy graph for this context's perspective player, or a
    /// shared empty graph when the session has no entry for that player.
    pub fn synergy_graph(&self) -> &SynergyGraph {
        self.session
            .synergy
            .get(&self.player)
            .unwrap_or_else(|| EMPTY_SYNERGY_GRAPH.get_or_init(SynergyGraph::empty))
    }
}
