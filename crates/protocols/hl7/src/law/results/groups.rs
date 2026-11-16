//! LAW result-group transitions preserve segment order and located failures.
use super::*;
use crate::law;

#[derive(Clone, Copy, PartialEq, Eq)]
enum PatientPhase {
    Absent,
    Patient,
    Visit,
}

pub(super) struct ResultGroups<'a> {
    specimen: Option<Segment<'a>>,
    container: Option<Segment<'a>>,
    pub(super) order: Option<Segment<'a>>,
    parent: Option<Parent>,
    pub(super) observations: Vec<Observation<'a>>,
    pub(super) specimen_observations: Vec<SpecimenObservation<'a>>,
    result_phase: ResultPhase,
    has_container_inventory: bool,
    pub(super) attached: Vec<Attached<'a>>,
    pub(super) order_status: Option<law::OrderStatus>,
    pub(super) order_count: usize,
    pub(super) observation_count: usize,
    patient_phase: PatientPhase,
    has_timing: bool,
}
impl<'a> ResultGroups<'a> {
    pub(super) fn new() -> Self {
        Self {
            specimen: None,
            container: None,
            order: None,
            parent: None,
            observations: Vec::new(),
            specimen_observations: Vec::new(),
            result_phase: ResultPhase::NoResult,
            has_container_inventory: false,
            attached: Vec::new(),
            order_status: None,
            order_count: 0usize,
            observation_count: 0usize,
            patient_phase: PatientPhase::Absent,
            has_timing: false,
        }
    }
    pub(super) fn parse_segment(
        &mut self,
        segment: Segment<'a>,
        ordinal: usize,
        profile: law::Profile<'_>,
    ) -> Result<(), Error> {
        match segment.id() {
            b"PID" => self.parse_pid(ordinal),
            b"PV1" => self.parse_pv1(segment),
            b"SPM" => self.parse_spm(segment, ordinal),
            b"SAC" => self.parse_sac(segment, ordinal),
            b"OBR" => self.parse_obr(segment, ordinal),
            b"ORC" => self.parse_orc(segment),
            b"TQ1" => self.parse_tq1(segment),
            b"OBX" => self.parse_obx(segment, ordinal),
            b"TCD" => self.parse_tcd(segment, profile),
            b"NTE" => self.parse_nte(segment),
            b"INV" => self.parse_inv(segment, profile),
            _ => Err(content("OUL", 0)),
        }
    }
    fn parse_pid(&mut self, ordinal: usize) -> Result<(), Error> {
        if (self.patient_phase != PatientPhase::Absent) || self.specimen.is_some() {
            return Err(content("PID", 0));
        }
        self.patient_phase = PatientPhase::Patient;
        self.parent = Some(Parent::Patient(ordinal));
        Ok(())
    }
    fn parse_pv1(&mut self, segment: Segment<'a>) -> Result<(), Error> {
        if (self.patient_phase == PatientPhase::Absent)
            || (self.patient_phase == PatientPhase::Visit)
            || self.specimen.is_some()
        {
            return Err(content("PV1", 0));
        }
        self.patient_phase = PatientPhase::Visit;
        self.attached.push(Attached {
            parent: self.parent.ok_or(content("PV1", 0))?,
            segment,
        });
        Ok(())
    }
    fn parse_spm(&mut self, segment: Segment<'a>, ordinal: usize) -> Result<(), Error> {
        if self.specimen.is_some() && self.order.is_none() {
            return Err(content("OBR", 0));
        }
        check_complete(self.order_status, self.observation_count)?;
        if self.order.is_some() && self.order_status.is_none() {
            return Err(content("ORC", 0));
        }
        self.specimen = Some(segment);
        self.container = None;
        self.order = None;
        self.order_status = None;
        self.parent = Some(Parent::Specimen(ordinal));
        Ok(())
    }
    fn parse_sac(&mut self, segment: Segment<'a>, ordinal: usize) -> Result<(), Error> {
        if self.specimen.is_none() || self.container.is_some() {
            return Err(content("SAC", 0));
        }
        self.container = Some(segment);
        self.has_container_inventory = false;
        self.order = None;
        self.parent = Some(Parent::Container(ordinal));
        Ok(())
    }
    fn parse_obr(&mut self, segment: Segment<'a>, ordinal: usize) -> Result<(), Error> {
        check_complete(self.order_status, self.observation_count)?;
        if self.order.is_some() && self.order_status.is_none() {
            return Err(content("ORC", 0));
        }
        if self.container.is_none() {
            return Err(content("OBR", 0));
        }
        for field in [2, 4] {
            if raw(segment, field).is_empty() {
                return Err(content("OBR", field));
            }
        }
        self.order = Some(segment);
        self.order_status = None;
        self.observation_count = 0;
        self.has_timing = false;
        self.order_count = self.order_count.checked_add(1).ok_or(content("OBR", 0))?;
        self.parent = Some(Parent::Order(ordinal));
        Ok(())
    }
    fn parse_orc(&mut self, segment: Segment<'a>) -> Result<(), Error> {
        if self.order.is_none()
            || law::OrderControl::parse(raw(segment, 1))? != law::OrderControl::StatusChange
            || self.order_status.is_some()
        {
            return Err(content("ORC", 1));
        }
        self.order_status = Some(law::OrderStatus::parse(raw(segment, 5))?);
        self.attached.push(Attached {
            parent: self.parent.ok_or(content("ORC", 0))?,
            segment,
        });
        Ok(())
    }
    fn parse_tq1(&mut self, segment: Segment<'a>) -> Result<(), Error> {
        if self.order_status.is_none() || self.has_timing || self.observation_count != 0 {
            return Err(content("TQ1", 0));
        }
        self.has_timing = true;
        self.attached.push(Attached {
            parent: self.parent.ok_or(content("TQ1", 0))?,
            segment,
        });
        Ok(())
    }
    fn parse_obx(&mut self, segment: Segment<'a>, ordinal: usize) -> Result<(), Error> {
        if self.order.is_none() && self.container.is_none() {
            let current_specimen = self.specimen.ok_or(content("SPM", 0))?;
            self.specimen_observations.push(SpecimenObservation {
                specimen: current_specimen,
                segment,
                values: values(segment)?,
            });
            self.parent = Some(Parent::SpecimenObservation(ordinal));
            return Ok(());
        }
        match self.order_status {
            Some(law::OrderStatus::InProgress | law::OrderStatus::Complete) => {}
            Some(law::OrderStatus::Scheduled | law::OrderStatus::Cancelled) | None => {
                return Err(content("OBX", 0));
            }
        }
        for field in [3, 4, 11] {
            if raw(segment, field).is_empty() {
                return Err(content("OBX", field));
            }
        }
        let sequence = self
            .observation_count
            .checked_add(1)
            .ok_or(content("OBX", 1))?;
        if raw(segment, 1) != sequence.to_string().as_bytes() {
            return Err(content("OBX", 1));
        }
        let observation = Observation {
            order_ordinal: self.order_count,
            segment,
            order: self.order.ok_or(content("OBR", 0))?,
            specimen: self.specimen.ok_or(content("SPM", 0))?,
            container: self.container.ok_or(content("SAC", 0))?,
            status: ResultStatus::parse(raw(segment, 11))?,
            detail: None,
            values: values(segment)?,
        };
        self.observations.push(observation);
        self.result_phase = ResultPhase::Observation;
        self.observation_count = self
            .observation_count
            .checked_add(1)
            .ok_or(content("OBX", 0))?;
        self.parent = Some(Parent::Observation(ordinal));
        Ok(())
    }
    fn parse_tcd(&mut self, segment: Segment<'a>, profile: law::Profile<'_>) -> Result<(), Error> {
        match self.result_phase {
            ResultPhase::Observation => {}
            ResultPhase::NoResult
            | ResultPhase::Detail
            | ResultPhase::Inventory
            | ResultPhase::Notes => return Err(content("TCD", 0)),
        }
        self.result_phase = ResultPhase::Detail;
        if !matches!(self.parent, Some(Parent::Observation(_))) {
            return Err(content("TCD", 0));
        }
        let observation = self.observations.last().ok_or(content("TCD", 0))?;
        if !law::order::same_test(raw(segment, 1), observation.code()) {
            return Err(Error::Inconsistent {
                segment: "TCD",
                field: 1,
            });
        }
        self.observations
            .last_mut()
            .ok_or(content("TCD", 0))?
            .detail = Some(law::TestDetail::from_validated(
            segment,
            profile,
            law::ControlDirection::Receive,
        ));
        self.attached.push(Attached {
            parent: self.parent.ok_or(content("TCD", 0))?,
            segment,
        });
        Ok(())
    }
    fn parse_nte(&mut self, segment: Segment<'a>) -> Result<(), Error> {
        match self.parent {
            Some(Parent::Patient(_)) => {}
            Some(Parent::Order(_)) => {
                if self.order_status.is_none() {
                    return Err(content("ORC", 0));
                }
            }
            Some(Parent::Observation(_)) => {
                self.result_phase = ResultPhase::Notes;
            }
            Some(Parent::Specimen(_) | Parent::Container(_) | Parent::SpecimenObservation(_))
            | None => return Err(content("NTE", 0)),
        }
        self.attached.push(Attached {
            parent: self.parent.ok_or(content("NTE", 0))?,
            segment,
        });
        Ok(())
    }
    fn parse_inv(&mut self, segment: Segment<'a>, profile: law::Profile<'_>) -> Result<(), Error> {
        match self.parent {
            Some(Parent::Container(_)) => {
                if self.has_container_inventory
                    || self.specimen.is_none_or(|s| {
                        law::control::part(raw(s, 11), 1) != b"Q"
                            || law::control::part(raw(s, 11), 3) != b"HL70369"
                    })
                {
                    return Err(content("INV", 0));
                }
                if profile.results.contributing_substances
                    && (law::control::part(raw(segment, 3), 1) != b"CO"
                        || law::control::part(raw(segment, 3), 3) != b"HL70384")
                {
                    return Err(content("INV", 3));
                }
                self.has_container_inventory = true;
            }
            Some(Parent::Observation(_)) => {
                match self.result_phase {
                    ResultPhase::Observation | ResultPhase::Detail | ResultPhase::Inventory => {}
                    ResultPhase::NoResult | ResultPhase::Notes => {
                        return Err(content("INV", 0));
                    }
                }
                self.result_phase = ResultPhase::Inventory;
            }
            Some(
                Parent::Patient(_)
                | Parent::Specimen(_)
                | Parent::Order(_)
                | Parent::SpecimenObservation(_),
            )
            | None => return Err(content("INV", 0)),
        }
        self.attached.push(Attached {
            parent: self.parent.ok_or(content("INV", 0))?,
            segment,
        });
        Ok(())
    }
}
