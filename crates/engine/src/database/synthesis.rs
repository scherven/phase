use std::str::FromStr;

use crate::database::mtgjson::{parse_mtgjson_mana_cost, AtomicCard};
use crate::game::printed_cards::derive_colors_from_mana_cost;
use crate::parser::oracle::parse_oracle_text;
use crate::types::ability::{
    AbilityCondition, AbilityCost, AbilityDefinition, AbilityKind, AdditionalCost,
    ContinuousModification, ControllerRef, Duration, Effect, FilterProp, ManaProduction,
    NinjutsuVariant, PtValue, QuantityExpr, RuntimeHandler, StaticDefinition, TargetFilter,
    TriggerCondition, TriggerDefinition, TypeFilter, TypedFilter,
};
use crate::types::card::{CardFace, CardLayout};
use crate::types::card_type::{CardType, CoreType, Supertype};
use crate::types::keywords::{Keyword, PartnerType};
use crate::types::mana::{ManaColor, ManaCost};
use crate::types::phase::Phase;
use crate::types::triggers::TriggerMode;
use crate::types::zones::Zone;

// ---------------------------------------------------------------------------
// Shared helpers for building card faces from MTGJSON data
// ---------------------------------------------------------------------------

/// Internal layout classification from MTGJSON layout strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutKind {
    Single,
    Split,
    Flip,
    Transform,
    Meld,
    Adventure,
    Modal,
}

pub fn map_layout(layout_str: &str) -> LayoutKind {
    match layout_str {
        "normal" | "saga" | "class" | "case" | "leveler" => LayoutKind::Single,
        "split" => LayoutKind::Split,
        "flip" => LayoutKind::Flip,
        "transform" => LayoutKind::Transform,
        "meld" => LayoutKind::Meld,
        "adventure" => LayoutKind::Adventure,
        "modal_dfc" => LayoutKind::Modal,
        _ => LayoutKind::Single,
    }
}

pub fn build_card_type(mtgjson: &AtomicCard) -> CardType {
    let supertypes = mtgjson
        .supertypes
        .iter()
        .filter_map(|s| Supertype::from_str(s).ok())
        .collect();
    let core_types = mtgjson
        .types
        .iter()
        .filter_map(|s| CoreType::from_str(s).ok())
        .collect();
    let subtypes = mtgjson.subtypes.clone();
    CardType {
        supertypes,
        core_types,
        subtypes,
    }
}

pub fn map_mtgjson_color(code: &str) -> Option<ManaColor> {
    match code {
        "W" => Some(ManaColor::White),
        "U" => Some(ManaColor::Blue),
        "B" => Some(ManaColor::Black),
        "R" => Some(ManaColor::Red),
        "G" => Some(ManaColor::Green),
        _ => None,
    }
}

pub fn parse_pt_value(s: &str) -> PtValue {
    match s.parse::<i32>() {
        Ok(n) => PtValue::Fixed(n),
        Err(_) => PtValue::Variable(s.to_string()),
    }
}

pub fn layout_faces(layout: &CardLayout) -> Vec<&CardFace> {
    match layout {
        CardLayout::Single(face) => vec![face],
        CardLayout::Split(a, b)
        | CardLayout::Flip(a, b)
        | CardLayout::Transform(a, b)
        | CardLayout::Meld(a, b)
        | CardLayout::Adventure(a, b)
        | CardLayout::Modal(a, b)
        | CardLayout::Omen(a, b) => vec![a, b],
        CardLayout::Specialize(base, variants) => {
            let mut faces = vec![base];
            faces.extend(variants);
            faces
        }
    }
}

// ---------------------------------------------------------------------------
// Synthesize functions — keyword → ability/trigger expansion
// ---------------------------------------------------------------------------

pub fn synthesize_basic_land_mana(face: &mut CardFace) {
    let land_mana: Vec<(&str, ManaColor)> = vec![
        ("Plains", ManaColor::White),
        ("Island", ManaColor::Blue),
        ("Swamp", ManaColor::Black),
        ("Mountain", ManaColor::Red),
        ("Forest", ManaColor::Green),
    ];

    for (subtype, color) in land_mana {
        if face.card_type.subtypes.iter().any(|s| s == subtype) {
            face.abilities.push(
                AbilityDefinition::new(
                    AbilityKind::Activated,
                    Effect::Mana {
                        produced: ManaProduction::Fixed {
                            colors: vec![color],
                        },
                        restrictions: vec![],
                        grants: vec![],
                        expiry: None,
                    },
                )
                .cost(AbilityCost::Tap),
            );
        }
    }
}

pub fn synthesize_equip(face: &mut CardFace) {
    let equip_abilities: Vec<AbilityDefinition> = face
        .keywords
        .iter()
        .filter_map(|kw| {
            if let Keyword::Equip(cost) = kw {
                Some(
                    AbilityDefinition::new(
                        AbilityKind::Activated,
                        Effect::Attach {
                            target: TargetFilter::Typed(
                                TypedFilter::creature().controller(ControllerRef::You),
                            ),
                        },
                    )
                    .cost(AbilityCost::Mana { cost: cost.clone() }),
                )
            } else {
                None
            }
        })
        .collect();

    face.abilities.extend(equip_abilities);
}

/// CR 702.49: Synthesize marker activated abilities for the Ninjutsu family
/// (Ninjutsu, Sneak, WebSlinging). The actual activation is handled by the
/// GameAction::ActivateNinjutsu path, not by normal activated ability resolution.
pub fn synthesize_ninjutsu_family(face: &mut CardFace) {
    let abilities: Vec<AbilityDefinition> = face
        .keywords
        .iter()
        .filter_map(|kw| {
            let (variant, cost) = match kw {
                Keyword::Ninjutsu(c) => (NinjutsuVariant::Ninjutsu, c),
                Keyword::CommanderNinjutsu(c) => (NinjutsuVariant::CommanderNinjutsu, c),
                Keyword::Sneak(c) => (NinjutsuVariant::Sneak, c),
                Keyword::WebSlinging(c) => (NinjutsuVariant::WebSlinging, c),
                _ => return None,
            };
            Some(
                AbilityDefinition::new(
                    AbilityKind::Activated,
                    Effect::RuntimeHandled {
                        handler: RuntimeHandler::NinjutsuFamily,
                    },
                )
                .cost(AbilityCost::NinjutsuFamily {
                    variant,
                    mana_cost: cost.clone(),
                }),
            )
        })
        .collect();
    face.abilities.extend(abilities);
}

// Warp is handled at runtime via Keyword::Warp(ManaCost):
// - `prepare_spell_cast` overrides the mana cost when cast from hand
// - `stack.rs::resolve_top` creates a delayed exile trigger on resolution

/// Synthesize Mobilize N trigger: when this creature attacks, create N 1/1 red
/// Warrior creature tokens tapped and attacking. Sacrifice them at end of combat.
pub fn synthesize_mobilize(face: &mut CardFace) {
    use crate::types::ability::PtValue;
    use crate::types::triggers::TriggerMode;

    for kw in &face.keywords {
        if let Keyword::Mobilize(qty) = kw {
            let token_effect = Effect::Token {
                name: "Warrior".to_string(),
                power: PtValue::Fixed(1),
                toughness: PtValue::Fixed(1),
                types: vec!["Creature".to_string(), "Warrior".to_string()],
                colors: vec![ManaColor::Red],
                keywords: vec![],
                tapped: true,
                count: qty.clone(),
                owner: TargetFilter::Controller,
                attach_to: None,
                enters_attacking: true,
                supertypes: vec![],
                static_abilities: vec![],
                enter_with_counters: vec![],
            };

            face.triggers.push(
                TriggerDefinition::new(TriggerMode::Attacks)
                    .execute(
                        AbilityDefinition::new(AbilityKind::Spell, token_effect)
                            .duration(Duration::UntilEndOfCombat),
                    )
                    .description(
                        "Mobilize — create Warrior tokens tapped and attacking".to_string(),
                    ),
            );
        }
    }
}

/// If the card has Changeling as a printed keyword, emit a characteristic-defining
/// static ability that grants all creature types (expanded at runtime via
/// `GameState::all_creature_types`).
pub fn synthesize_changeling_cda(face: &mut CardFace) {
    if face
        .keywords
        .iter()
        .any(|k| matches!(k, Keyword::Changeling))
    {
        face.static_abilities.push(
            StaticDefinition::continuous()
                .affected(TargetFilter::SelfRef)
                .modifications(vec![ContinuousModification::AddAllCreatureTypes])
                .cda(),
        );
    }
}

/// Synthesize `additional_cost` from `Keyword::Kicker(ManaCost)`.
///
/// If the card has Kicker and no additional_cost was already parsed from Oracle text
/// (blight takes precedence since it's parsed from the "as an additional cost" line),
/// set `additional_cost = Some(AdditionalCost::Optional(AbilityCost::Mana { cost }))`.
pub fn synthesize_kicker(face: &mut CardFace) {
    if face.additional_cost.is_some() {
        return;
    }
    if let Some(cost) = face.keywords.iter().find_map(|k| match k {
        Keyword::Kicker(cost) => Some(cost.clone()),
        _ => None,
    }) {
        face.additional_cost = Some(AdditionalCost::Optional(AbilityCost::Mana { cost }));
    }
}

/// Synthesize Gift optional cost and delivery effect.
/// Gift is a promise (zero-cost optional additional cost) that sets `additional_cost_paid`
/// when the player promises the gift. Conditional branches ("if the gift was promised" /
/// "wasn't promised") are handled by the parser via `strip_additional_cost_conditional`.
///
/// Gift delivery (opponent receives the gift) is injected as a `GiftDelivery` effect
/// wrapping the first spell ability. The delivery checks `additional_cost_paid` at
/// resolution time — if the gift wasn't promised, it's a no-op and the spell resolves
/// normally. If promised, the opponent receives the gift before the spell's other effects.
pub fn synthesize_gift(face: &mut CardFace) {
    if face.additional_cost.is_some() {
        return;
    }
    // Use rfind (last match) because the MTGJSON bare "Gift" keyword defaults to
    // Gift(Card), while the Oracle-parsed keyword (e.g., Gift(TappedFish)) comes later
    // and is always the correct, specific kind.
    let gift_kind = face.keywords.iter().rev().find_map(|k| match k {
        Keyword::Gift(kind) => Some(kind.clone()),
        _ => None,
    });
    let Some(gift_kind) = gift_kind else {
        return;
    };

    // Gift uses a zero-cost optional additional cost — the "cost" is just a decision.
    face.additional_cost = Some(AdditionalCost::Optional(AbilityCost::Mana {
        cost: ManaCost::zero(),
    }));

    // Inject GiftDelivery as a wrapper around the first spell ability.
    // The delivery effect is a no-op when the gift wasn't promised, so the
    // chain always flows through to the spell's normal effects.
    if let Some(first_ability) = face.abilities.first_mut() {
        let original = std::mem::replace(
            first_ability,
            AbilityDefinition::new(AbilityKind::Spell, Effect::GiftDelivery { kind: gift_kind }),
        );
        first_ability.sub_ability = Some(Box::new(original));
    }
}

/// CR 719.2: Synthesize the intrinsic Case auto-solve trigger.
/// Every Case with a solve condition has: "At the beginning of your end step,
/// if this Case is not solved and its requirement is met, it becomes solved."
pub fn synthesize_case_solve(face: &mut CardFace) {
    if !face.card_type.subtypes.iter().any(|s| s == "Case") {
        return;
    }
    if face.solve_condition.is_none() {
        return;
    }

    face.triggers.push(
        TriggerDefinition::new(TriggerMode::Phase)
            .phase(Phase::End)
            .execute(AbilityDefinition::new(
                AbilityKind::Spell,
                Effect::SolveCase,
            ))
            .condition(TriggerCondition::SolveConditionMet)
            .description("CR 719.2: Case auto-solve at end step".to_string()),
    );
}

/// CR 702.87a: Synthesize level up activated ability — "Pay {cost}: Put a level counter
/// on this permanent. Activate only as a sorcery."
pub fn synthesize_level_up(face: &mut CardFace) {
    use crate::types::ability::ActivationRestriction;

    let level_up_abilities: Vec<AbilityDefinition> = face
        .keywords
        .iter()
        .filter_map(|kw| {
            if let Keyword::LevelUp(cost) = kw {
                // CR 702.87a: Level up is an activated ability, sorcery-speed only.
                Some(
                    AbilityDefinition::new(
                        AbilityKind::Activated,
                        Effect::PutCounter {
                            counter_type: "level".to_string(),
                            count: QuantityExpr::Fixed { value: 1 },
                            target: TargetFilter::SelfRef,
                        },
                    )
                    .cost(AbilityCost::Mana { cost: cost.clone() })
                    .activation_restrictions(vec![ActivationRestriction::AsSorcery]),
                )
            } else {
                None
            }
        })
        .collect();

    face.abilities.extend(level_up_abilities);
}

/// Brawl variant of CR 903.3: determine if a card can be a Brawl commander.
/// Uses the union of MTGJSON's `leadershipSkills.brawl` (which catches Vehicles/Spacecraft)
/// and our own type-line check (legendary creature or legendary planeswalker, or
/// "can be your commander" in Oracle text).
pub fn compute_brawl_commander(mtgjson: &super::mtgjson::AtomicCard, face: &CardFace) -> bool {
    // Source 1: MTGJSON leadership skills (catches Legendary Vehicles etc.)
    let mtgjson_says = mtgjson
        .leadership_skills
        .as_ref()
        .is_some_and(|ls| ls.brawl);

    // Source 2: type-line analysis
    let is_legendary = face.card_type.supertypes.contains(&Supertype::Legendary);
    let is_creature = face.card_type.core_types.contains(&CoreType::Creature);
    let is_planeswalker = face.card_type.core_types.contains(&CoreType::Planeswalker);
    let explicitly_allowed = face
        .oracle_text
        .as_ref()
        .is_some_and(|text| text.to_ascii_lowercase().contains("can be your commander"));
    let type_line_says = (is_legendary && (is_creature || is_planeswalker)) || explicitly_allowed;

    mtgjson_says || type_line_says
}

/// CR 702.29a/e: Synthesize Cycling and Typecycling keywords into activated abilities.
///
/// Cycling: "[Cost], Discard this card: Draw a card." (activated from hand)
/// Typecycling: "[Cost], Discard this card: Search library for a [type] card,
///   reveal it, put it into your hand. Then shuffle."
pub fn synthesize_cycling(face: &mut CardFace) {
    let cycling_abilities: Vec<AbilityDefinition> = face
        .keywords
        .iter()
        .filter_map(|kw| match kw {
            // CR 702.29a: Basic cycling — discard self, draw a card.
            Keyword::Cycling(cost) => {
                let composite_cost = AbilityCost::Composite {
                    costs: vec![
                        AbilityCost::Mana { cost: cost.clone() },
                        // CR 702.29a: "Discard THIS card" — self_ref = true.
                        AbilityCost::Discard {
                            count: 1,
                            filter: None,
                            random: false,
                            self_ref: true,
                        },
                    ],
                };
                let mut def = AbilityDefinition::new(
                    AbilityKind::Activated,
                    Effect::Draw {
                        count: QuantityExpr::Fixed { value: 1 },
                    },
                )
                .cost(composite_cost);
                def.activation_zone = Some(Zone::Hand);
                Some(def)
            }
            // CR 702.29e: Typecycling — discard self, search library for [type] card.
            Keyword::Typecycling { cost, subtype } => {
                let composite_cost = AbilityCost::Composite {
                    costs: vec![
                        AbilityCost::Mana { cost: cost.clone() },
                        AbilityCost::Discard {
                            count: 1,
                            filter: None,
                            random: false,
                            self_ref: true,
                        },
                    ],
                };
                let filter = typecycling_subtype_to_filter(subtype);
                let shuffle_def = AbilityDefinition::new(
                    AbilityKind::Spell,
                    Effect::Shuffle {
                        target: TargetFilter::Controller,
                    },
                );
                let mut def = AbilityDefinition::new(
                    AbilityKind::Activated,
                    Effect::SearchLibrary {
                        filter,
                        count: QuantityExpr::Fixed { value: 1 },
                        reveal: true,
                        target_player: None,
                    },
                )
                .cost(composite_cost);
                def.activation_zone = Some(Zone::Hand);
                def.sub_ability = Some(Box::new(shuffle_def));
                Some(def)
            }
            _ => None,
        })
        .collect();

    face.abilities.extend(cycling_abilities);
}

/// CR 702.97a: Synthesize Scavenge into an activated ability on the card.
///
/// Scavenge is an activated ability that functions only while the card with scavenge is
/// in a graveyard. "Scavenge [cost]" means "[Cost], Exile this card from your graveyard:
/// Put a number of +1/+1 counters equal to this card's power on target creature. Activate
/// only as a sorcery."
///
/// Power snapshot timing (CR 208.3 + CR 400.7): At resolution the source has already
/// been exiled as a cost; CR 702.97a specifies "the power of the card you exiled",
/// which is read from the exile-zone object via `QuantityRef::SelfPower` (with LKI
/// fallback if the object is somehow gone). Non-battlefield zones do not run layer
/// computation, so the read value equals the card's printed power — the correct
/// target for "this card's power" in the graveyard reminder text. No new quantity
/// ref is needed; `SelfPower` is already the right abstraction.
pub fn synthesize_scavenge(face: &mut CardFace) {
    use crate::types::ability::{ActivationRestriction, QuantityRef};

    let scavenge_abilities: Vec<AbilityDefinition> = face
        .keywords
        .iter()
        .filter_map(|kw| {
            let Keyword::Scavenge(cost) = kw else {
                return None;
            };
            // CR 118.3: Composite cost — pay mana, then exile this card from graveyard.
            let composite_cost = AbilityCost::Composite {
                costs: vec![
                    AbilityCost::Mana { cost: cost.clone() },
                    // CR 702.97a: "Exile this card from your graveyard" — SelfRef + Graveyard
                    // is auto-paid by pay_ability_cost (no player choice needed).
                    AbilityCost::Exile {
                        count: 1,
                        zone: Some(Zone::Graveyard),
                        filter: Some(TargetFilter::SelfRef),
                    },
                ],
            };
            // CR 702.97a: "Put a number of +1/+1 counters equal to this card's power on
            // target creature." SelfPower is resolved via LKI at resolution time so the
            // power read is the card's last known power before it was exiled.
            let effect = Effect::PutCounter {
                counter_type: "P1P1".to_string(),
                count: QuantityExpr::Ref {
                    qty: QuantityRef::SelfPower,
                },
                target: TargetFilter::Typed(TypedFilter::new(TypeFilter::Creature)),
            };
            let mut def = AbilityDefinition::new(AbilityKind::Activated, effect)
                .cost(composite_cost)
                // CR 702.97a: "Activate only as a sorcery."
                .sorcery_speed()
                .activation_restrictions(vec![ActivationRestriction::AsSorcery]);
            // CR 702.97a: "functions only while the card with scavenge is in a graveyard."
            def.activation_zone = Some(Zone::Graveyard);
            Some(def)
        })
        .collect();

    face.abilities.extend(scavenge_abilities);
}

/// Convert a typecycling subtype string to a `TargetFilter` for library search.
///
/// Single subtypes (e.g., "Plains", "Forest") → subtype filter.
/// "Basic Land" → supertype Basic + core type Land.
fn typecycling_subtype_to_filter(subtype: &str) -> TargetFilter {
    if subtype == "Basic Land" {
        TargetFilter::Typed(TypedFilter::new(TypeFilter::Land).properties(vec![
            FilterProp::HasSupertype {
                value: Supertype::Basic,
            },
        ]))
    } else {
        TargetFilter::Typed(TypedFilter::card().subtype(subtype.to_string()))
    }
}

/// CR 702.153a: Synthesize Casualty N into an optional sacrifice cost + self-cast copy trigger.
///
/// Casualty N = two abilities:
/// 1. Optional additional cost: sacrifice a creature with power N or greater
/// 2. Triggered ability: "When you cast this spell, if a casualty cost was paid, copy it"
pub fn synthesize_casualty(face: &mut CardFace) {
    let threshold = match face.keywords.iter().find_map(|k| match k {
        Keyword::Casualty(n) => Some(*n),
        _ => None,
    }) {
        Some(n) => n,
        None => return,
    };

    // CR 702.153a: "As an additional cost, you may sacrifice a creature with power N or greater"
    if face.additional_cost.is_none() {
        let sacrifice_filter =
            TargetFilter::Typed(
                TypedFilter::creature().properties(vec![FilterProp::PowerGE {
                    value: QuantityExpr::Fixed {
                        value: threshold as i32,
                    },
                }]),
            );
        face.additional_cost = Some(AdditionalCost::Optional(AbilityCost::Sacrifice {
            target: sacrifice_filter,
            count: 1,
        }));
    }

    // CR 702.153a: "When you cast this spell, if a casualty cost was paid, copy it.
    // If the spell has any targets, you may choose new targets for the copy."
    let copy_effect = AbilityDefinition::new(
        AbilityKind::Spell,
        Effect::CopySpell {
            target: TargetFilter::SelfRef,
        },
    )
    .condition(AbilityCondition::AdditionalCostPaid);

    face.triggers.push(
        TriggerDefinition::new(TriggerMode::SpellCast)
            .valid_card(TargetFilter::SelfRef)
            .trigger_zones(vec![Zone::Stack])
            .execute(copy_effect)
            .description("Casualty — copy this spell when cast with casualty paid".to_string()),
    );
}

/// CR 702.42a: Synthesize Entwine cost onto modal spell's ModalChoice.
///
/// Sets `entwine_cost` on the face's modal abilities and raises `max_choices`
/// to `mode_count` so all modes can be selected.
pub fn synthesize_entwine(face: &mut CardFace) {
    let cost = match face.keywords.iter().find_map(|k| match k {
        Keyword::Entwine(cost) => Some(cost.clone()),
        _ => None,
    }) {
        Some(c) => c,
        None => return,
    };

    // Set entwine_cost on the face's modal choice + allow all-mode selection
    if let Some(ref mut modal) = face.modal {
        modal.entwine_cost = Some(cost);
        // CR 702.42a: "You may choose all modes" — raise max_choices to allow it
        modal.max_choices = modal.mode_count;
    }
}

/// Run all synthesis functions in canonical order on a card face.
/// Both `oracle_loader.rs` and `oracle_gen.rs` call this to ensure the same
/// complete set of synthesizers is applied.
pub fn synthesize_all(face: &mut CardFace) {
    synthesize_basic_land_mana(face);
    synthesize_equip(face);
    // CR 702.122a: Crew has no synthesized ability — activation is handled by
    // GameAction::CrewVehicle directly, not through ActivateAbility dispatch.
    // The Keyword::Crew(N) on the card provides display information.
    synthesize_ninjutsu_family(face);
    synthesize_changeling_cda(face);
    synthesize_kicker(face);
    synthesize_gift(face);
    synthesize_case_solve(face);
    // Warp: no synthesis needed — runtime handled by Keyword::Warp directly
    synthesize_mobilize(face);
    synthesize_level_up(face);
    synthesize_cycling(face);
    synthesize_scavenge(face);
    synthesize_casualty(face);
    synthesize_entwine(face);
}

/// Build a `CardFace` from MTGJSON data, running the Oracle text parser and all synthesis.
/// Both `oracle_loader.rs` and `oracle_gen.rs` call this to ensure identical processing.
pub fn build_oracle_face(mtgjson: &AtomicCard, oracle_id: Option<String>) -> CardFace {
    build_oracle_face_inner(mtgjson, oracle_id, false)
}

/// Build an Oracle face for a multi-face card, skipping MTGJSON keywords
/// to prevent cross-face keyword leakage (B8: Saga back-face keyword contamination).
pub fn build_oracle_face_multi(mtgjson: &AtomicCard, oracle_id: Option<String>) -> CardFace {
    build_oracle_face_inner(mtgjson, oracle_id, true)
}

fn build_oracle_face_inner(
    mtgjson: &AtomicCard,
    oracle_id: Option<String>,
    skip_mtgjson_keywords: bool,
) -> CardFace {
    let card_type = build_card_type(mtgjson);
    // Raw MTGJSON keyword names (lowercased) for keyword-only line detection.
    // Still needed for keyword line detection even when skipping MTGJSON keywords.
    let mtgjson_keyword_names: Vec<String> = mtgjson
        .keywords
        .as_ref()
        .map(|kws| kws.iter().map(|s| s.to_ascii_lowercase()).collect())
        .unwrap_or_default();
    let parser_keyword_names: Vec<String> = if skip_mtgjson_keywords {
        vec!["__force_keyword_extract__".to_string()]
    } else {
        mtgjson_keyword_names.clone()
    };

    // B8: For multi-face cards, skip MTGJSON-provided keywords entirely.
    // MTGJSON duplicates keywords across both faces of Transform/DFC cards,
    // causing the front face to incorrectly gain back-face keywords.
    // Parser-extracted keywords from `extract_keyword_line` are face-specific.
    let mut keywords: Vec<Keyword> = if skip_mtgjson_keywords {
        Vec::new()
    } else {
        mtgjson
            .keywords
            .as_ref()
            .map(|kws| {
                kws.iter()
                    .map(|s| s.parse::<Keyword>().unwrap())
                    .filter(|k| !matches!(k, Keyword::Unknown(_)))
                    .collect()
            })
            .unwrap_or_default()
    };

    let oracle_text = mtgjson.text.as_deref().unwrap_or("");
    let face_name = mtgjson.face_name.as_deref().unwrap_or(&mtgjson.name);

    let types: Vec<String> = mtgjson.types.clone();
    let subtypes: Vec<String> = mtgjson.subtypes.clone();

    let parsed = parse_oracle_text(
        oracle_text,
        face_name,
        &parser_keyword_names,
        &types,
        &subtypes,
    );

    // Merge keywords extracted from Oracle text with MTGJSON keywords.
    // When the Oracle parser extracts a parameterized keyword (e.g., Morph({2}{B}{G}{U})),
    // remove any MTGJSON-derived default of the same kind (e.g., Morph(zero)).
    for extracted_kw in &parsed.extracted_keywords {
        let kind = extracted_kw.kind();
        keywords.retain(|existing| existing.kind() != kind || existing == extracted_kw);
    }
    keywords.extend(parsed.extracted_keywords);

    // CR 702.124c: "Partner with [Name]" — upgrade Generic → With(name).
    // MTGJSON sends both "Partner" and "Partner with" keywords; the former produces
    // Partner(Generic) via FromStr. Scan Oracle text for the actual partner name.
    if mtgjson_keyword_names.contains(&"partner with".to_string()) {
        let lower_oracle = oracle_text.to_lowercase();
        if let Some(line) = lower_oracle
            .lines()
            .find(|l| l.starts_with("partner with "))
        {
            let rest = &line["partner with ".len()..];
            // Name ends at first '(' (reminder text) or end of line
            let name = rest.find('(').map(|i| &rest[..i]).unwrap_or(rest).trim();
            if !name.is_empty() {
                // Extract original-case name from the raw oracle text
                let original_name = mtgjson
                    .text
                    .as_deref()
                    .unwrap_or("")
                    .lines()
                    .find(|l| l.to_lowercase().starts_with("partner with "))
                    .map(|l| {
                        let r = &l["Partner with ".len()..];
                        r.find('(').map(|i| &r[..i]).unwrap_or(r).trim().to_string()
                    })
                    .unwrap_or_else(|| name.to_string());

                // Upgrade any Generic partner to With(name)
                for kw in &mut keywords {
                    if matches!(kw, Keyword::Partner(PartnerType::Generic)) {
                        *kw = Keyword::Partner(PartnerType::With(original_name.clone()));
                        break;
                    }
                }
            }
        }
    }

    // CR 702.124: Deduplicate — if any non-Generic partner variant exists,
    // remove stale Partner(Generic) entries (e.g., MTGJSON "Partner" keyword
    // producing Generic when Oracle text has "Partner—Friends forever").
    let has_specific_partner = keywords
        .iter()
        .any(|kw| matches!(kw, Keyword::Partner(pt) if !matches!(pt, PartnerType::Generic)));
    if has_specific_partner {
        keywords.retain(|kw| !matches!(kw, Keyword::Partner(PartnerType::Generic)));
    }

    // CR 702.11c: Deduplicate — if any HexproofFrom variant exists, remove
    // bare Hexproof (MTGJSON sends both "Hexproof" and "Hexproof from [quality]").
    let has_hexproof_from = keywords
        .iter()
        .any(|kw| matches!(kw, Keyword::HexproofFrom(_)));
    if has_hexproof_from {
        keywords.retain(|kw| !matches!(kw, Keyword::Hexproof));
    }

    let mana_cost = mtgjson
        .mana_cost
        .as_deref()
        .map(parse_mtgjson_mana_cost)
        .unwrap_or_default();

    let mana_derived_colors = derive_colors_from_mana_cost(&mana_cost);
    let mtgjson_colors: Vec<ManaColor> = mtgjson
        .colors
        .iter()
        .filter_map(|c| map_mtgjson_color(c))
        .collect();
    let color_override = if mtgjson_colors != mana_derived_colors {
        Some(mtgjson_colors)
    } else {
        None
    };

    let mut face = CardFace {
        name: face_name.to_string(),
        mana_cost,
        card_type,
        power: mtgjson.power.as_ref().map(|s| parse_pt_value(s)),
        toughness: mtgjson.toughness.as_ref().map(|s| parse_pt_value(s)),
        loyalty: mtgjson.loyalty.clone(),
        defense: mtgjson.defense.clone(),
        oracle_text: mtgjson.text.clone(),
        non_ability_text: None,
        flavor_name: None,
        keywords,
        abilities: parsed.abilities,
        triggers: parsed.triggers,
        static_abilities: parsed.statics,
        replacements: parsed.replacements,
        color_override,
        scryfall_oracle_id: oracle_id,
        modal: parsed.modal,
        additional_cost: parsed.additional_cost,
        strive_cost: parsed.strive_cost,
        casting_restrictions: parsed.casting_restrictions,
        casting_options: parsed.casting_options,
        solve_condition: parsed.solve_condition,
        parse_warnings: parsed.parse_warnings,
        brawl_commander: false,
        metadata: Default::default(),
    };

    face.brawl_commander = compute_brawl_commander(mtgjson, &face);
    synthesize_all(&mut face);
    face
}

#[cfg(test)]
mod scavenge_synthesis_tests {
    use super::*;
    use crate::types::ability::{ActivationRestriction, QuantityRef};
    use crate::types::mana::{ManaCost, ManaCostShard};

    fn face_with_scavenge(cost: ManaCost) -> CardFace {
        let mut face = CardFace::default();
        face.keywords.push(Keyword::Scavenge(cost));
        face
    }

    /// CR 702.97a: Scavenge synthesis produces exactly one activated ability whose
    /// shape matches the reminder text — graveyard activation, sorcery speed,
    /// composite cost of mana + self-exile, +1/+1 counters on target creature
    /// scaled by SelfPower.
    #[test]
    fn synthesize_scavenge_builds_activated_ability_with_correct_shape() {
        let cost = ManaCost::Cost {
            shards: vec![ManaCostShard::Green],
            generic: 3,
        };
        let mut face = face_with_scavenge(cost.clone());
        synthesize_scavenge(&mut face);

        assert_eq!(face.abilities.len(), 1, "exactly one scavenge ability");
        let def = &face.abilities[0];
        assert_eq!(def.kind, AbilityKind::Activated);
        assert_eq!(def.activation_zone, Some(Zone::Graveyard));
        assert!(def.sorcery_speed);
        assert!(def
            .activation_restrictions
            .contains(&ActivationRestriction::AsSorcery));

        // CR 118.3: Composite cost — mana + exile-self-from-graveyard.
        match def.cost.as_ref().expect("scavenge must have a cost") {
            AbilityCost::Composite { costs } => {
                assert_eq!(costs.len(), 2);
                assert!(matches!(&costs[0], AbilityCost::Mana { cost: c } if *c == cost));
                assert!(matches!(
                    &costs[1],
                    AbilityCost::Exile {
                        count: 1,
                        zone: Some(Zone::Graveyard),
                        filter: Some(TargetFilter::SelfRef),
                    }
                ));
            }
            other => panic!("expected Composite cost, got {:?}", other),
        }

        // CR 702.97a: Effect is +1/+1 counters equal to SelfPower on target creature.
        match def.effect.as_ref() {
            Effect::PutCounter {
                counter_type,
                count,
                target,
            } => {
                assert_eq!(counter_type, "P1P1");
                assert!(matches!(
                    count,
                    QuantityExpr::Ref {
                        qty: QuantityRef::SelfPower
                    }
                ));
                assert!(
                    matches!(target, TargetFilter::Typed(tf) if tf.type_filters.contains(&TypeFilter::Creature))
                );
            }
            other => panic!("expected PutCounter effect, got {:?}", other),
        }
    }

    /// Scavenge {0} (Slitherhead) — cost-0 mana still produces a well-formed ability.
    #[test]
    fn synthesize_scavenge_handles_zero_cost() {
        let cost = ManaCost::default();
        let mut face = face_with_scavenge(cost);
        synthesize_scavenge(&mut face);
        assert_eq!(face.abilities.len(), 1);
    }

    /// Cards without Scavenge are unaffected.
    #[test]
    fn synthesize_scavenge_is_noop_without_keyword() {
        let mut face = CardFace::default();
        face.keywords.push(Keyword::Flying);
        synthesize_scavenge(&mut face);
        assert!(face.abilities.is_empty());
    }
}

#[cfg(test)]
mod scavenge_runtime_tests {
    use super::*;
    use crate::game::casting::{can_activate_ability_now, handle_activate_ability};
    use crate::game::zones::create_object;
    use crate::types::counter::CounterType;
    use crate::types::game_state::GameState;
    use crate::types::identifiers::{CardId, ObjectId};
    use crate::types::mana::ManaCost;
    use crate::types::player::PlayerId;

    /// Helper: put a creature in the graveyard with Scavenge synthesized on it, and
    /// stage a target creature on the battlefield. Returns (source_id, target_id).
    fn setup_scavenge_scenario(
        state: &mut GameState,
        scavenge_cost: ManaCost,
    ) -> (ObjectId, ObjectId) {
        let source = create_object(
            state,
            CardId(1),
            PlayerId(0),
            "Scavenge Source".to_string(),
            Zone::Graveyard,
        );
        {
            let obj = state.objects.get_mut(&source).unwrap();
            obj.power = Some(4);
            obj.toughness = Some(4);
            obj.card_types.core_types.push(CoreType::Creature);
            obj.keywords.push(Keyword::Scavenge(scavenge_cost.clone()));
        }
        // Synthesize to attach the activated ability.
        let mut face = CardFace::default();
        face.keywords.push(Keyword::Scavenge(scavenge_cost));
        synthesize_scavenge(&mut face);
        state
            .objects
            .get_mut(&source)
            .unwrap()
            .abilities
            .extend(face.abilities);

        let target = create_object(
            state,
            CardId(2),
            PlayerId(0),
            "Target Creature".to_string(),
            Zone::Battlefield,
        );
        {
            let obj = state.objects.get_mut(&target).unwrap();
            obj.power = Some(2);
            obj.toughness = Some(2);
            obj.card_types.core_types.push(CoreType::Creature);
        }

        (source, target)
    }

    /// CR 702.97a: Scavenge can be activated while the source is in a graveyard.
    /// CR 702.97a: Activation is gated by sorcery timing.
    #[test]
    fn scavenge_is_activatable_from_graveyard_at_sorcery_speed() {
        let mut state = GameState::new_two_player(42);
        // Active player's main phase, empty stack — sorcery-speed window.
        state.active_player = PlayerId(0);
        state.phase = Phase::PreCombatMain;

        let zero_cost = ManaCost::default(); // Scavenge {0}
        let (source, _target) = setup_scavenge_scenario(&mut state, zero_cost);

        assert!(
            can_activate_ability_now(&state, PlayerId(0), source, 0),
            "Scavenge {{0}} should be activatable from graveyard during sorcery window"
        );
    }

    /// CR 702.97a: Scavenge cannot be activated at instant speed.
    #[test]
    fn scavenge_rejects_instant_speed() {
        let mut state = GameState::new_two_player(42);
        state.active_player = PlayerId(0);
        // Outside the sorcery window (upkeep phase is not a main phase).
        state.phase = Phase::Upkeep;

        let (source, _target) = setup_scavenge_scenario(&mut state, ManaCost::default());

        assert!(
            !can_activate_ability_now(&state, PlayerId(0), source, 0),
            "Scavenge must reject activation outside the sorcery-speed window"
        );
    }

    /// CR 602.1: Scavenge can only be activated while the source is in the graveyard.
    #[test]
    fn scavenge_rejects_from_battlefield() {
        let mut state = GameState::new_two_player(42);
        state.active_player = PlayerId(0);
        state.phase = Phase::PreCombatMain;

        let (source, _target) = setup_scavenge_scenario(&mut state, ManaCost::default());
        // Move source out of graveyard onto the battlefield.
        crate::game::zones::move_to_zone(&mut state, source, Zone::Battlefield, &mut Vec::new());

        assert!(
            !can_activate_ability_now(&state, PlayerId(0), source, 0),
            "Scavenge must reject activation when source is not in a graveyard"
        );
    }

    /// CR 702.97a + CR 208.3: End-to-end — activating Scavenge exiles the source from
    /// graveyard as a cost, then on resolution places +1/+1 counters equal to SelfPower
    /// (read via LKI) on target creature.
    #[test]
    fn scavenge_activation_exiles_source_and_places_counters_on_target() {
        use crate::game::stack::resolve_top;

        let mut state = GameState::new_two_player(42);
        state.active_player = PlayerId(0);
        state.phase = Phase::PreCombatMain;
        // Use Scavenge {0} (Slitherhead-shaped) to avoid mana-pool plumbing in the test.
        let (source, target) = setup_scavenge_scenario(&mut state, ManaCost::default());

        // Activate the ability.
        let mut events = Vec::new();
        let result = handle_activate_ability(&mut state, PlayerId(0), source, 0, &mut events);
        assert!(result.is_ok(), "activation must succeed: {:?}", result);

        // CR 702.97a: Exile cost — source moved graveyard → exile as cost payment.
        assert_eq!(
            state.objects[&source].zone,
            Zone::Exile,
            "Scavenge source must be exiled as a cost"
        );
        assert!(
            !state.players[0].graveyard.contains(&source),
            "source must be removed from graveyard"
        );
        assert!(
            state.exile.contains(&source),
            "source must be in exile zone"
        );

        // Ability is on the stack awaiting resolution.
        assert!(!state.stack.is_empty(), "ability must be on the stack");

        let mut resolve_events = Vec::new();
        resolve_top(&mut state, &mut resolve_events);

        // CR 702.97a + CR 208.3: target creature gains counters equal to source's LKI power (4).
        let counter_count = state.objects[&target]
            .counters
            .get(&CounterType::Plus1Plus1)
            .copied()
            .unwrap_or(0);
        assert_eq!(
            counter_count, 4,
            "target must gain +1/+1 counters equal to source's LKI power (4)"
        );
    }
}
