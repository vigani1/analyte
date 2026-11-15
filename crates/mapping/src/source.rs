//! Source adapters preserve parser boundaries; mapping never splits wire text.
use crate::FieldSyntax;
use crate::{Error, Position};
use analyte_config::{Field as Selection, Group};

#[derive(Clone, Copy)]
pub(crate) enum Record<'a> {
    Astm(&'a analyte_astm::Record),
    Hl7(analyte_hl7::Segment<'a>),
}

pub(crate) struct Node<'a> {
    pub record: Record<'a>,
    pub name: String,
    pub parent: Option<usize>,
    pub group: Group,
    pub comments: Vec<usize>,
}

pub(crate) struct Source<'a> {
    pub nodes: Vec<Node<'a>>,
    pub syntax: FieldSyntax,
    pub component_separator: u8,
    pub charset: analyte_hl7::Charset,
}

impl Source<'_> {
    pub(crate) fn attach_comments(&mut self) -> Result<(), Error> {
        for index in 0..self.nodes.len() {
            let parent = self
                .nodes
                .get(index)
                .filter(|n| n.name == "C" || n.name == "NTE")
                .and_then(|n| n.parent);
            if let Some(parent) = parent {
                let position = self.position(index, 0);
                let owner = self.nodes.get_mut(parent).ok_or(Error::Structure {
                    position,
                    reason: "comment parent missing",
                })?;
                owner.comments.push(index);
            }
        }
        Ok(())
    }

    pub(crate) fn raw_size(&self, index: usize) -> usize {
        self.nodes.get(index).map_or(0, |node| match node.record {
            Record::Astm(record) => record.raw().len(),
            Record::Hl7(segment) => segment.raw().len(),
        })
    }

    pub(crate) fn position(&self, index: usize, field: usize) -> Position {
        Position {
            record: index.saturating_add(1),
            field,
        }
    }

    pub(crate) fn text(&self, raw: &[u8], index: usize, field: usize) -> Result<String, Error> {
        if raw.len() > 16384 {
            return Err(Error::Limit {
                position: self.position(index, field),
                resource: "source text",
            });
        }
        match self.charset {
            analyte_hl7::Charset::Ascii | analyte_hl7::Charset::Utf8 => std::str::from_utf8(raw)
                .map(str::to_owned)
                .map_err(|_| Error::Encoding {
                    position: self.position(index, field),
                }),
            analyte_hl7::Charset::Latin1 => Ok(raw.iter().map(|byte| char::from(*byte)).collect()),
        }
    }

    pub(crate) fn raw(&self, index: usize, field: usize) -> Result<Option<String>, Error> {
        let Some(node) = self.nodes.get(index) else {
            return Ok(None);
        };
        let raw = match node.record {
            Record::Astm(record) => record.field(field).map(analyte_astm::Field::raw),
            Record::Hl7(segment) => segment.field(field).map(analyte_hl7::Field::raw),
        };
        raw.map(|bytes| self.text(bytes, index, field)).transpose()
    }

    pub(crate) fn component(
        &self,
        index: usize,
        field: usize,
        repetition: usize,
        component: usize,
    ) -> Result<Option<String>, Error> {
        let Some(node) = self.nodes.get(index) else {
            return Ok(None);
        };
        let Some(repetition) = repetition.checked_sub(1) else {
            return Ok(None);
        };
        let Some(component) = component.checked_sub(1) else {
            return Ok(None);
        };
        let bytes = match node.record {
            Record::Astm(record) => record
                .field(field)
                .and_then(|f| f.repeats().get(repetition))
                .and_then(|r| r.get(component))
                .map(analyte_astm::Component::raw),
            Record::Hl7(segment) => segment
                .field(field)
                .and_then(|f| f.repetitions().nth(repetition))
                .and_then(|r| r.components().nth(component))
                .map(analyte_hl7::Component::raw),
        };
        bytes
            .map(|bytes| self.text(bytes, index, field))
            .transpose()
    }

    pub(crate) fn repetitions(&self, index: usize, field: usize) -> Result<Vec<String>, Error> {
        let Some(node) = self.nodes.get(index) else {
            return Ok(Vec::new());
        };
        match node.record {
            Record::Astm(record) => record.field(field).map_or_else(
                || Ok(Vec::new()),
                |f| {
                    let mut values = Vec::new();
                    for repetition in f.repeats() {
                        if values.len() == 256 {
                            return Err(Error::Limit {
                                position: self.position(index, field),
                                resource: "repeated values",
                            });
                        }
                        let mut raw = Vec::new();
                        for (ordinal, component) in repetition.iter().enumerate() {
                            if ordinal != 0 {
                                raw.push(self.component_separator);
                            }
                            if raw
                                .len()
                                .checked_add(component.raw().len())
                                .is_none_or(|length| length > 16384)
                            {
                                return Err(Error::Limit {
                                    position: self.position(index, field),
                                    resource: "source text",
                                });
                            }
                            raw.extend_from_slice(component.raw());
                        }
                        values.push(self.text(&raw, index, field)?);
                    }
                    Ok(values)
                },
            ),
            Record::Hl7(segment) => segment.field(field).map_or_else(
                || Ok(Vec::new()),
                |f| {
                    let mut values = Vec::new();
                    for repetition in f.repetitions() {
                        if values.len() == 256 {
                            return Err(Error::Limit {
                                position: self.position(index, field),
                                resource: "repeated values",
                            });
                        }
                        values.push(self.text(repetition.raw(), index, field)?);
                    }
                    Ok(values)
                },
            ),
        }
    }

    pub(crate) fn ancestors(&self, index: usize) -> Result<Vec<usize>, Error> {
        let mut path = Vec::new();
        let mut current = Some(index);
        while let Some(index) = current {
            if path.len() == 32 {
                return Err(Error::Limit {
                    position: self.position(index, 0),
                    resource: "hierarchy depth",
                });
            }
            let node = self.nodes.get(index).ok_or(Error::Structure {
                position: self.position(index, 0),
                reason: "missing parent",
            })?;
            path.push(index);
            current = node.parent;
        }
        path.reverse();
        Ok(path)
    }

    pub(crate) fn selection_position(
        &self,
        index: usize,
        selection: &Selection,
    ) -> Result<Position, Error> {
        for ancestor in self.ancestors(index)? {
            if self.nodes.get(ancestor).is_some_and(|node| {
                node.group == selection.group && node.name == selection.record.as_str()
            }) {
                return Ok(self.position(ancestor, usize::from(selection.field)));
            }
        }
        Ok(self.position(index, usize::from(selection.field)))
    }

    pub(crate) fn selected(
        &self,
        index: usize,
        selection: &Selection,
    ) -> Result<Option<String>, Error> {
        let mut found = None;
        for ancestor in self.ancestors(index)? {
            let Some(node) = self.nodes.get(ancestor) else {
                continue;
            };
            if node.group == selection.group && node.name == selection.record.as_str() {
                if found.is_some() {
                    return Err(Error::Structure {
                        position: self.position(index, usize::from(selection.field)),
                        reason: "ambiguous group-relative selection",
                    });
                }
                found = Some(ancestor);
            }
        }
        match found {
            Some(ancestor) => self.component(
                ancestor,
                usize::from(selection.field),
                usize::from(selection.repetition),
                usize::from(selection.component),
            ),
            None => Ok(None),
        }
    }
}

pub(crate) fn astm(message: &analyte_astm::Message) -> Source<'_> {
    use analyte_astm::Kind;
    let nodes = message
        .records()
        .iter()
        .map(|record| {
            let (name, group) = match record.kind() {
                Kind::Header => ("H", Group::Message),
                Kind::Patient => ("P", Group::Patient),
                Kind::Order => ("O", Group::Order),
                Kind::Result => ("R", Group::Observation),
                Kind::Comment => ("C", Group::Observation),
                Kind::Query => ("Q", Group::Message),
                Kind::Terminator => ("L", Group::Message),
                Kind::Scientific => ("S", Group::Patient),
                Kind::Manufacturer => ("M", Group::Observation),
            };
            Node {
                record: Record::Astm(record),
                name: name.into(),
                parent: record.parent(),
                group,
                comments: Vec::new(),
            }
        })
        .collect();
    Source {
        nodes,
        syntax: FieldSyntax::AstmLis02,
        component_separator: message.delimiters().bytes().get(2).copied().unwrap_or(b'^'),
        charset: analyte_hl7::Charset::Latin1,
    }
}

pub(crate) fn hl7(message: &analyte_hl7::Message) -> Result<Source<'_>, Error> {
    let msh = analyte_hl7::Msh::new(message).map_err(|source| Error::Hl7 {
        position: Position {
            record: 1,
            field: 9,
        },
        source,
    })?;
    let parts: Vec<_> = msh.message_type().collect();
    let oul = match parts.as_slice() {
        [b"ORU", b"R01"] | [b"ORU", b"R01", b"ORU_R01"] => false,
        [b"OUL", b"R22"] | [b"OUL", b"R22", b"OUL_R22"]
            if msh.version() == analyte_hl7::Version::V251 =>
        {
            true
        }
        _ => {
            return Err(Error::Structure {
                position: Position {
                    record: 1,
                    field: 9,
                },
                reason: "only ORU R01 and edition 2.5.1 OUL R22 are supported",
            });
        }
    };
    let mut source = Source {
        nodes: Vec::new(),
        syntax: FieldSyntax::Hl7V2,
        component_separator: message.delimiters().bytes().get(1).copied().unwrap_or(b'^'),
        charset: message.charset(),
    };
    let mut patient = Some(0);
    let mut specimen = None;
    let mut control = None;
    let mut order = None;
    let mut note_parent = Some(0);
    for (index, segment) in message.segments().enumerate() {
        let position = Position {
            record: index.saturating_add(1),
            field: 0,
        };
        let name = std::str::from_utf8(segment.id()).map_err(|_| Error::Encoding { position })?;
        let (parent, group) = match name {
            "MSH" if index == 0 => (None, Group::Message),
            "PID" => {
                patient = Some(index);
                specimen = None;
                control = None;
                order = None;
                (Some(0), Group::Patient)
            }
            "ORC" => {
                let parent = if oul {
                    Some(order.ok_or(Error::Structure {
                        position,
                        reason: "OUL ORC must follow OBR",
                    })?)
                } else {
                    order = None;
                    patient
                };
                control = Some(index);
                (parent, Group::Order)
            }
            "OBR" => {
                if oul && specimen.is_none() {
                    return Err(Error::Structure {
                        position,
                        reason: "OUL order has no specimen group",
                    });
                }
                let parent = if oul {
                    control = None;
                    specimen
                } else {
                    control.or(patient)
                };
                if !oul {
                    control = None;
                }
                order = Some(index);
                (parent, Group::Order)
            }
            "SPM" => {
                if oul {
                    control = None;
                    order = None;
                }
                let parent = if oul {
                    patient
                } else {
                    Some(order.ok_or(Error::Structure {
                        position,
                        reason: "ORU specimen group has no order",
                    })?)
                };
                specimen = Some(index);
                (parent, Group::Order)
            }
            "SAC" if specimen.is_some() => (specimen, Group::Order),
            "OBX" => {
                let parent = if oul {
                    control.or(order).or(specimen)
                } else {
                    match (order, specimen) {
                        (Some(o), Some(s)) if s > o => Some(s),
                        (Some(o), Some(_)) | (Some(o), None) => Some(o),
                        (None, Some(s)) => Some(s),
                        (None, None) => None,
                    }
                };
                (
                    Some(parent.ok_or(Error::Structure {
                        position,
                        reason: "observation has no order or specimen group",
                    })?),
                    Group::Observation,
                )
            }
            "NTE" | "TCD" | "SID" => {
                if name != "NTE"
                    && (!oul
                        || !note_parent
                            .and_then(|i| source.nodes.get(i))
                            .is_some_and(|n| n.name == "OBX"))
                {
                    return Err(Error::Structure {
                        position,
                        reason: "test detail or substance is outside an OUL result group",
                    });
                }
                (note_parent, Group::Observation)
            }
            "PV1" | "PV2" => (patient, Group::Patient),
            _ => {
                return Err(Error::Structure {
                    position,
                    reason: "unsupported or misplaced segment; retain receipt for review",
                });
            }
        };
        source.nodes.push(Node {
            record: Record::Hl7(segment),
            name: name.into(),
            parent,
            group,
            comments: Vec::new(),
        });
        if name != "NTE" && name != "TCD" && name != "SID" {
            note_parent = Some(index);
        }
    }
    Ok(source)
}
