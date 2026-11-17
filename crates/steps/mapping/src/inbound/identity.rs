//! Resolves result identities using configured sample and source evidence only.
use crate::RecordSettings;
use crate::inbound::source::Source;
use crate::{Error, Output, Problem, finding, identifier, text};
use analyte_config::{NoSubId, OrderNamespace, SubId};
use analyte_model::{NamespacedId, ObservationIdentity, SampleId, SampleIdentity, SampleSource};

pub(crate) fn sample(
    source: &Source<'_>,
    index: usize,
    config: &RecordSettings<'_>,
    output: &mut Output,
) -> Result<Option<SampleIdentity>, Error> {
    let selection = &config.mappings.sample;
    let position = source.selection_position(index, &selection.field)?;
    let Some(value) = source.present(source.selected(index, &selection.field)?) else {
        finding(output, position, Problem::MissingSample)?;
        return Ok(None);
    };
    let source = match selection.source {
        analyte_config::SampleSource::Barcode => SampleSource::ScannedByMachine,
        analyte_config::SampleSource::HostOrder => SampleSource::WorkList,
        analyte_config::SampleSource::Operator => SampleSource::TypedOnMachine,
        analyte_config::SampleSource::Unknown => SampleSource::Unknown,
    };
    Ok(Some(SampleIdentity {
        id: SampleId::new(&value).map_err(|source| Error::Text { position, source })?,
        source,
        position: None,
    }))
}

pub(crate) fn observation(
    source: &Source<'_>,
    index: usize,
    config: &RecordSettings<'_>,
    code: NamespacedId,
    output: &mut Output,
    law: Option<&std::collections::BTreeMap<usize, &analyte_hl7::law::Observation<'_>>>,
) -> Result<Option<ObservationIdentity>, Error> {
    if let Some(observations) = law {
        let observation = observations.get(&index).copied();
        let Some(observation) = observation else {
            finding(
                output,
                source.position(index, 0),
                Problem::MissingIdentity {
                    setting: "LAW specimen observation has no AWOS",
                },
            )?;
            return Ok(None);
        };
        return law_observation(source, index, config, code, output, observation);
    }
    let settings = &config.mappings.identity;
    let mut missing = false;
    let mut required = |field: &analyte_config::Field, setting| -> Result<String, Error> {
        match source.present(source.selected(index, field)?) {
            Some(value) => Ok(value),
            None => {
                missing = true;
                finding(
                    output,
                    source.selection_position(index, field)?,
                    Problem::MissingIdentity { setting },
                )?;
                Ok(String::new())
            }
        }
    };
    let order_id = required(&settings.order_id, "order_id")?;
    let namespace = match &settings.order_namespace {
        OrderNamespace::Fixed { fixed } => fixed.as_str().to_owned(),
        OrderNamespace::Field { field } => required(field, "order_namespace")?,
    };
    let sub_id = match &settings.sub_id {
        SubId::Field(field) => required(field, "sub_id")?,
        SubId::None(NoSubId::None) => String::new(),
    };
    let mut run = Vec::new();
    for field in &settings.run {
        run.push(required(field, "run")?);
    }
    if missing {
        return Ok(None);
    }
    let position = source.position(index, 3);
    Ok(Some(ObservationIdentity {
        code,
        sub_id: text(&sub_id, position)?,
        run: crate::identity_run(&run.iter().map(String::as_str).collect::<Vec<_>>())
            .map_err(|source| Error::Text { position, source })?,
        order: NamespacedId {
            value: identifier(
                &order_id,
                source.position(index, usize::from(settings.order_id.field)),
            )?,
            namespace: identifier(&namespace, position)?,
        },
    }))
}

// TF-2b 3.29.4/B.3: preserve the complete OG, not just its first component.
// No downloaded-order lookup is required for analyzer-created unsolicited work.
fn law_observation(
    source: &Source<'_>,
    index: usize,
    config: &RecordSettings<'_>,
    code: NamespacedId,
    output: &mut Output,
    observation: &analyte_hl7::law::Observation<'_>,
) -> Result<Option<ObservationIdentity>, Error> {
    let order = source
        .ancestors(index)?
        .into_iter()
        .rev()
        .find(|i| source.nodes.get(*i).is_some_and(|node| node.name == "OBR"));
    let Some(order) = order else {
        finding(
            output,
            source.position(index, 0),
            Problem::MissingIdentity {
                setting: "LAW order",
            },
        )?;
        return Ok(None);
    };
    let field = if observation.awos().is_some() { 2 } else { 3 };
    let id = source.present(source.component(order, field, 1, 1)?);
    let namespace = source.present(source.component(order, field, 1, 2)?);
    let universal = source.present(source.component(order, field, 1, 3)?);
    let kind = source.present(source.component(order, field, 1, 4)?);
    let namespace = if field == 2 {
        // TF-2b C.6-2: received EI.2+ are X, never assigning authority.
        match &config.mappings.identity.order_namespace {
            OrderNamespace::Fixed { fixed } => Some(fixed.as_str().to_owned()),
            OrderNamespace::Field { .. } => None,
        }
    } else if universal.is_some() {
        Some(format!(
            "{}^{}^{}",
            namespace.unwrap_or_default(),
            universal.unwrap_or_default(),
            kind.unwrap_or_default()
        ))
    } else {
        namespace
    };
    let sub_id = source.present(Some(source.text(observation.sub_id(), index, 4)?));
    let (Some(id), Some(namespace), Some(sub_id)) = (id, namespace, sub_id) else {
        finding(
            output,
            source.position(order, field),
            Problem::MissingIdentity {
                setting: "LAW AWOS/analyzer order authority or OG",
            },
        )?;
        return Ok(None);
    };
    let position = source.position(order, field);
    Ok(Some(ObservationIdentity {
        code,
        sub_id: text(&sub_id, source.position(index, 4))?,
        run: text("", position)?,
        order: NamespacedId {
            value: identifier(&id, position)?,
            namespace: identifier(&namespace, position)?,
        },
    }))
}
