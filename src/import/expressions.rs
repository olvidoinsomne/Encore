//! Interchange directions collected before source voices map to native layers.
use super::{Warnings, annotations::SourceAnchor, child, number, text, value};
use crate::{
    document::Anchor,
    expressions::{
        Expression, ExpressionKind, Hairpin, HairpinKind, Placement, TempoChange, TempoRampEnd,
        TimePoint,
    },
    model::Score,
};
use roxmltree::Node;
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct Expressions {
    marks: Vec<(SourceAnchor, Expression)>,
    lines: Vec<(SourceAnchor, SourceAnchor, Hairpin)>,
    open: BTreeMap<String, (SourceAnchor, Hairpin)>,
    tempo: Vec<TempoChange>,
    tempo_open: BTreeMap<String, usize>,
}
fn dummy(at: &SourceAnchor) -> Anchor {
    Anchor {
        voice: 0,
        part: 0,
        measure: at.measure,
        tick: at.tick,
        layer: 0,
    }
}
impl Expressions {
    pub(super) fn xml(
        &mut self,
        node: Node<'_, '_>,
        at: SourceAnchor,
        bpm: Option<u16>,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        let placement = if node.attribute("placement") == Some("above") {
            Placement::Above
        } else {
            Placement::Below
        };
        let offset = [
            node.attribute("relative-x")
                .map(str::parse::<f32>)
                .transpose()
                .map_err(|_| "Invalid expression offset.")?
                .unwrap_or(0.0),
            -node
                .attribute("relative-y")
                .map(str::parse::<f32>)
                .transpose()
                .map_err(|_| "Invalid expression offset.")?
                .unwrap_or(0.0),
        ];
        let automatic_dynamic = node.descendants().any(|node| {
            node.has_tag_name("other-direction") && node.text() == Some("encore-derived-dynamic")
        });
        let initial_tempo = node.descendants().any(|node| {
            node.has_tag_name("other-direction") && node.text() == Some("encore-initial-tempo")
        });
        let custom_expression = node
            .descendants()
            .filter(|node| node.has_tag_name("other-direction"))
            .filter_map(|node| node.text())
            .find_map(|text| text.strip_prefix("encore-expression:"))
            .map(serde_json::from_str::<Expression>)
            .transpose()
            .map_err(|_| "Invalid Encore expression metadata.")?;
        let custom_line = node
            .descendants()
            .filter(|node| node.has_tag_name("other-direction"))
            .filter_map(|node| node.text())
            .find_map(|text| text.strip_prefix("encore-hairpin:"))
            .map(serde_json::from_str::<Hairpin>)
            .transpose()
            .map_err(|_| "Invalid Encore hairpin metadata.")?;
        if let Some(mark) = &custom_expression {
            self.marks.push((at.clone(), mark.clone()));
        }
        for dynamic in node.descendants().filter(|n| {
            n.has_tag_name("dynamics") && custom_expression.is_none() && !automatic_dynamic
        }) {
            if let Some(mark) = dynamic.children().find(|n| n.is_element()) {
                let label = if mark.has_tag_name("other-dynamics") {
                    text(mark)
                } else {
                    mark.tag_name().name().into()
                };
                let explicit_velocity = node
                    .descendants()
                    .find(|n| n.has_tag_name("sound"))
                    .and_then(|n| n.attribute("dynamics"))
                    .map(|value| {
                        value
                            .parse::<f64>()
                            .map(|value| (value * 0.9).round().clamp(0.0, 127.0) as u8)
                    })
                    .transpose()
                    .map_err(|_| "Invalid dynamic playback value.")?;
                let mut expression =
                    Expression::dynamic(dummy(&at), label, explicit_velocity.unwrap_or(80));
                expression.velocity = explicit_velocity;
                expression.placement = placement;
                expression.offset = offset;
                self.marks.push((at.clone(), expression));
            }
        }
        let navigation = node.descendants().any(|n| {
            n.has_tag_name("sound")
                && ["dacapo", "dalsegno", "segno", "coda", "fine", "tocoda"]
                    .iter()
                    .any(|name| n.attribute(*name).is_some())
        });
        let words = node
            .descendants()
            .filter(|n| n.has_tag_name("words"))
            .map(text)
            .collect::<Vec<_>>()
            .join(" ");
        if bpm.is_none() && !navigation && custom_expression.is_none() {
            for element in node
                .descendants()
                .filter(|n| n.has_tag_name("words") || n.has_tag_name("rehearsal"))
            {
                let mut expression = Expression::text(dummy(&at), text(element));
                expression.kind = if element.has_tag_name("rehearsal") {
                    ExpressionKind::Rehearsal
                } else if element
                    .attribute("id")
                    .is_some_and(|id| id.starts_with("encore-technique"))
                {
                    ExpressionKind::Technique
                } else {
                    ExpressionKind::Text
                };
                expression.system = element.has_tag_name("rehearsal")
                    || element
                        .attribute("id")
                        .is_some_and(|id| id.contains("system"));
                expression.placement = if node.attribute("placement") == Some("below") {
                    Placement::Below
                } else {
                    Placement::Above
                };
                expression.offset = offset;
                self.marks.push((at.clone(), expression));
            }
        }
        for wedge in node.descendants().filter(|n| n.has_tag_name("wedge")) {
            let key = format!("{}:{}", at.staff, wedge.attribute("number").unwrap_or("1"));
            match wedge.attribute("type") {
                Some("crescendo" | "diminuendo") => {
                    let mut line = custom_line
                        .clone()
                        .unwrap_or_else(|| Hairpin::new(dummy(&at), dummy(&at)));
                    line.kind = if wedge.attribute("type") == Some("crescendo") {
                        HairpinKind::Crescendo
                    } else {
                        HairpinKind::Diminuendo
                    };
                    if custom_line.is_none() {
                        line.placement = placement;
                        line.offset = offset[1];
                    }
                    if wedge.attribute("niente") == Some("yes") {
                        line.start_velocity = Some(0);
                    }
                    if wedge
                        .attribute("line-type")
                        .is_some_and(|line| line != "solid")
                    {
                        warnings.add("Dashed hairpins use a solid wedge in Encore.");
                    }
                    if self.open.insert(key, (at.clone(), line)).is_some() {
                        return Err("Overlapping MusicXML wedge identifiers.".into());
                    }
                }
                Some("stop") => {
                    let (start, mut line) = self
                        .open
                        .remove(&key)
                        .ok_or("A hairpin stop has no matching start.")?;
                    if wedge.attribute("niente") == Some("yes") {
                        line.end_velocity = Some(0);
                    }
                    self.lines.push((start, at.clone(), line));
                }
                Some("continue") => (),
                _ => return Err("Invalid MusicXML hairpin type.".into()),
            }
        }
        let dashes = node.descendants().find(|n| n.has_tag_name("dashes"));
        if let Some(bpm) = bpm.filter(|_| !initial_tempo) {
            let at = TimePoint {
                measure: at.measure,
                tick: at.tick,
            };
            if let Some(dashes) = dashes {
                let key = dashes.attribute("number").unwrap_or("1").to_owned();
                if dashes.attribute("type") == Some("stop") {
                    let index = self
                        .tempo_open
                        .remove(&key)
                        .ok_or("Tempo ramp end has no matching start.")?;
                    self.tempo[index].ramp = Some(TempoRampEnd { end: at, bpm });
                } else {
                    let index = self.put_tempo(TempoChange {
                        at,
                        bpm,
                        ramp: None,
                        text: words,
                    });
                    self.tempo_open.insert(key, index);
                }
            } else {
                self.put_tempo(TempoChange {
                    at,
                    bpm,
                    ramp: None,
                    text: words,
                });
            }
        }
        Ok(())
    }
    fn put_tempo(&mut self, change: TempoChange) -> usize {
        if let Some(index) = self.tempo.iter().position(|prior| prior.at == change.at) {
            self.tempo[index] = change;
            index
        } else {
            self.tempo.push(change);
            self.tempo.len() - 1
        }
    }
    pub(super) fn muse(
        &mut self,
        node: Node<'_, '_>,
        at: SourceAnchor,
        division: u32,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        match node.tag_name().name(){
            "Tempo"=>{let raw=number::<f64>(node,"tempo")?.ok_or("Missing MuseScore tempo.")?*60.0;if !raw.is_finite()||!(20.0..=400.0).contains(&raw){return Err("Tempo must be 20–400 BPM.".into());}self.put_tempo(TempoChange{at:TimePoint{measure:at.measure,tick:at.tick},bpm:raw.round() as u16,ramp:None,text:child(node,"text").map(text).unwrap_or_default()});},
            "Dynamic"=>{let label=value(node,"subtype").unwrap_or("mf");let velocity=number::<u8>(node,"velocity")?.filter(|velocity|*velocity>0).unwrap_or_else(||super::musicxml::dynamic_velocity(label));let mut expression=Expression::dynamic(dummy(&at),label,velocity);expression.velocity=None;self.marks.push((at.clone(),expression));},
            "StaffText"|"SystemText"|"RehearsalMark"=>{if child(node,"swing").is_some(){return Ok(());}let mut mark=Expression::text(dummy(&at),child(node,"text").map(text).unwrap_or_default());mark.kind=if node.has_tag_name("RehearsalMark"){ExpressionKind::Rehearsal}else{ExpressionKind::Text};mark.system = !node.has_tag_name("StaffText");if !mark.text.trim().is_empty(){self.marks.push((at,mark));}},
            "Spanner" if node.attribute("type")==Some("HairPin")||node.attribute("type")==Some("Hairpin")=>{let data=child(node,"HairPin").or_else(||child(node,"Hairpin")).unwrap_or(node);let location=child(node,"next").and_then(|next|child(next,"location")).ok_or("MuseScore hairpin has no endpoint.")?;let measure_delta=number::<i32>(location,"measures")?.unwrap_or(0);let fraction=value(location,"fractions").map(super::musescore::fraction_ticks).transpose()?.unwrap_or(0);let mut end=at.clone();end.measure=(at.measure as i64+i64::from(measure_delta)).try_into().map_err(|_|"Invalid hairpin measure endpoint.")?;end.tick=(i64::from(at.tick)+fraction).try_into().map_err(|_|"Invalid hairpin beat endpoint.")?;let mut line=Hairpin::new(dummy(&at),dummy(&end));line.kind=if number::<u8>(data,"subtype")?.unwrap_or(0)==1{HairpinKind::Diminuendo}else{HairpinKind::Crescendo};self.lines.push((at,end,line));},
            "HairPin"|"Hairpin"=>{let ticks=number::<u64>(node,"ticks")?.ok_or("MuseScore legacy hairpin requires explicit duration ticks.")?;let scaled=ticks.checked_mul(480).ok_or("Hairpin duration overflow.")?;if scaled%u64::from(division)!=0{return Err("Hairpin endpoint is off Encore's timing grid.".into());}let mut end=at.clone();end.tick=end.tick.checked_add((scaled/u64::from(division)).try_into().map_err(|_|"Hairpin duration exceeds score bounds.")?).ok_or("Hairpin duration overflow.")?;let mut line=Hairpin::new(dummy(&at),dummy(&end));line.kind=if number::<u8>(node,"subtype")?.unwrap_or(0)==1{HairpinKind::Diminuendo}else{HairpinKind::Crescendo};self.lines.push((at,end,line));},
            "Spanner" if node.attribute("type")==Some("GradualTempoChange")=>warnings.add("MuseScore gradual tempo curves are not yet retained; explicit tempo marks are preserved."),_=>()
        }
        Ok(())
    }
    pub(super) fn shift(&mut self, measure: usize, ticks: u32) {
        for (at, _) in &mut self.marks {
            if at.measure == measure {
                at.tick += ticks;
            }
        }
        for (start, end, _) in &mut self.lines {
            if start.measure == measure {
                start.tick += ticks;
            }
            if end.measure == measure {
                end.tick += ticks;
            }
        }
        for change in &mut self.tempo {
            if change.at.measure == measure {
                change.at.tick += ticks;
            }
            if let Some(ramp) = &mut change.ramp
                && ramp.end.measure == measure
            {
                ramp.end.tick += ticks;
            }
        }
    }
    pub(super) fn resolve(
        self,
        score: &mut Score,
        mapping: &super::VoiceMapping,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        let width = score.measure_ticks();
        let count = score.measure_count();
        let at = |source: &SourceAnchor| -> Result<Anchor, String> {
            let target = mapping
                .get(&(source.staff, source.voice.clone()))
                .or_else(|| {
                    mapping
                        .iter()
                        .find(|((staff, _), _)| *staff == source.staff)
                        .map(|(_, value)| value)
                })
                .ok_or("An expression references a missing source staff/voice.")?;
            let absolute = source.measure as u64 * u64::from(width) + u64::from(source.tick);
            let (measure, tick) = if absolute == count as u64 * u64::from(width) {
                (count - 1, width)
            } else {
                (
                    (absolute / u64::from(width)) as usize,
                    (absolute % u64::from(width)) as u32,
                )
            };
            Ok(Anchor {
                voice: 0,
                part: target.part,
                measure,
                tick,
                layer: target.layer,
            })
        };
        for (source, mut mark) in self.marks {
            mark.anchor = at(&source)?;
            if !mark.system
                || !score.notation.expressions.iter().any(|prior| {
                    prior.system
                        && prior.kind == mark.kind
                        && prior.text == mark.text
                        && prior.anchor.measure == mark.anchor.measure
                        && prior.anchor.tick == mark.anchor.tick
                })
            {
                score.notation.expressions.push(mark);
            }
        }
        for (start, end, mut line) in self.lines {
            line.start = at(&start)?;
            line.end = at(&end)?;
            score.notation.spanners.push(line);
        }
        if !self.open.is_empty() {
            return Err("A hairpin is missing its closing endpoint.".into());
        }
        if !self.tempo_open.is_empty() {
            warnings.add("An unfinished tempo dashes line is retained as its initial tempo mark.");
        }
        for mut change in self.tempo {
            let width = score.measure_ticks();
            if change.at.tick >= width {
                change.at.measure += (change.at.tick / width) as usize;
                change.at.tick %= width;
            }
            if let Some(prior) = score
                .notation
                .tempo
                .iter()
                .find(|prior| prior.at == change.at)
            {
                if prior.bpm != change.bpm || prior.ramp != change.ramp {
                    return Err("Source staves disagree on tempo playback.".into());
                }
            } else {
                score.notation.tempo.push(change);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        expressions::TempoMap,
        model::{Duration, Note, Pitch},
    };
    #[test]
    fn musicxml_preserves_explicit_expressions_hairpin_playback_and_tempo_ramp() {
        let mut source = Score::blank();
        source.parts.truncate(1);
        source.parts[0].measures[0].notes = vec![Note {
            voice: 0,
            voice_launch: None,
            start: 0,
            duration: Duration::Whole,
            dotted: false,
            tuplet: None,
            layer: 0,
            pitches: vec![Pitch::from_midi(60)],
            velocity: 80,
            staccato: false,
            articulations: vec![],
            tied: false,
        }];
        let start = Anchor {
            voice: 0,
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        let end = Anchor {
            measure: 1,
            ..start
        };
        source.notation.expressions = vec![
            Expression::dynamic(start, "p", 48),
            Expression {
                kind: ExpressionKind::Technique,
                text: "dolce & legato".into(),
                placement: Placement::Above,
                offset: [3.0, -4.0],
                ..Expression::text(Anchor { tick: 480, ..start }, "unused")
            },
        ];
        source.notation.spanners = vec![Hairpin {
            start_velocity: Some(48),
            end_velocity: Some(96),
            offset: 4.0,
            ..Hairpin::new(start, end)
        }];
        source.notation.tempo = vec![TempoChange {
            at: start.into(),
            bpm: 120,
            ramp: Some(TempoRampEnd {
                end: end.into(),
                bpm: 60,
            }),
            text: "rit.".into(),
        }];
        source.validate().unwrap();
        let xml = crate::export::musicxml(&source);
        let document = super::super::parse_xml(&xml).unwrap();
        let imported = super::super::musicxml::parse(&document).unwrap().score;
        assert_eq!(imported.notation.expressions, source.notation.expressions);
        assert_eq!(imported.notation.spanners, source.notation.spanners);
        assert_eq!(imported.notation.tempo, source.notation.tempo);
        assert!(
            (TempoMap::new(&imported).unwrap().duration_seconds()
                - TempoMap::new(&source).unwrap().duration_seconds())
            .abs()
                < 1e-9
        );
    }
    #[test]
    fn musescore_pickup_slur_keeps_the_next_measure_target_on_its_downbeat() {
        let xml = "<museScore version='4.30'><Score><Staff id='1'><Measure len='1/4'><voice><TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig><Chord><durationType>quarter</durationType><Spanner type='Slur'><next><location><measures>1</measures></location></next></Spanner><Note><pitch>60</pitch></Note></Chord></voice></Measure><Measure><voice><Chord><durationType>quarter</durationType><Note><pitch>62</pitch></Note></Chord></voice></Measure></Staff></Score></museScore>";
        let document = super::super::parse_xml(xml).unwrap();
        let imported = super::super::musescore::parse(&document).unwrap();
        assert_eq!(imported.score.notation.slurs.len(), 1);
        let slur = &imported.score.notation.slurs[0];
        assert_eq!((slur.start.measure, slur.start.tick), (0, 1440));
        assert_eq!((slur.end.measure, slur.end.tick), (1, 0));
    }
}
