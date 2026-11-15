//! Only configured identity and sample evidence can resolve a result.
use crate::source::Source;
use crate::{Error, Output, Problem, finding, identifier, present, text};
use analyte_config::Validated;
use analyte_config::{NoSubId, OrderNamespace, SubId};
use analyte_model::*;

pub(crate) fn sample(
    source: &Source<'_>,
    index: usize,
    config: &Validated,
    output: &mut Output,
) -> Result<Option<SampleIdentity>, Error> {
    let selection = &config.effective().machine().mappings.sample;
    let position = source.selection_position(index, &selection.field)?;
    let Some(value) = present(source.selected(index, &selection.field)?) else {
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
    config: &Validated,
    code: NamespacedId,
    output: &mut Output,
) -> Result<Option<ObservationIdentity>, Error> {
    let settings = &config.effective().machine().mappings.identity;
    let mut missing = false;
    let mut required = |field: &analyte_config::Field, setting| -> Result<String, Error> {
        match present(source.selected(index, field)?) {
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
    let mut run = String::new();
    for field in &settings.run {
        let value = required(field, "run")?;
        let component = format!("{}:{value}", value.len());
        if run
            .len()
            .checked_add(component.len())
            .is_none_or(|length| length > 16384)
        {
            return Err(Error::Limit {
                position: source.position(index, usize::from(field.field)),
                resource: "run identity",
            });
        }
        run.push_str(&component);
    }
    if missing {
        return Ok(None);
    }
    let position = source.position(index, 3);
    Ok(Some(ObservationIdentity {
        code,
        sub_id: text(&sub_id, position)?,
        run: text(&run, position)?,
        order: NamespacedId {
            value: identifier(
                &order_id,
                source.position(index, usize::from(settings.order_id.field)),
            )?,
            namespace: identifier(&namespace, position)?,
        },
    }))
}
