//! Shows retention policy and committed purge evidence and applies audited operator holds.
use super::Cursor;
use crate::admin::Error;
use crate::admin::markup::Markup;
use crate::admin::{
    server::Context,
    sessions::Session,
    view::{self, Tab, count, format},
};
use crate::deadline;
use analyte_model::{Identifier, Reason};
use analyte_store::{Audit, HoldTarget};
use axum::{
    Extension, Form,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;

fn page(body: &Markup) -> Response {
    Html(
        view::page(Tab::DataKeeping, "Data keeping")
            .markup(concat!(
                "<p class=\"lead\" title=\"Retention and holds\">How long Analyte keeps ",
                "received messages, and holds that keep chosen data until someone releases ",
                "them.</p>",
            ))
            .fragment(body)
            .markup(view::END),
    )
    .into_response()
}
fn failure(error: &dyn std::error::Error) -> Response {
    (
        StatusCode::CONFLICT,
        Html(view::message(Tab::DataKeeping, "Hold not saved", error)),
    )
        .into_response()
}
pub(in crate::admin) async fn show(
    State(state): State<Arc<Context>>,
    Query(cursor): Query<Cursor>,
    session: Option<Extension<Arc<Session>>>,
) -> Response {
    match render(
        &state,
        session.as_ref().map(|Extension(s)| s.as_ref()),
        cursor.after,
    )
    .await
    {
        Ok(body) => page(&body),
        Err(error) => failure(&error),
    }
}
async fn render(state: &Context, session: Option<&Session>, after: u64) -> Result<Markup, Error> {
    let after = i64::try_from(after)?;
    let (last, oldest, holds) = state
        .store
        .request::<_, crate::admin::error::CommandError>(deadline()?, move |store| {
            Ok::<_, crate::admin::error::CommandError>((
                store.last_purge_run()?,
                store.oldest_retained_receipt()?,
                store.retention_holds(after)?,
            ))
        })
        .await?;
    let now = jiff::Timestamp::now();
    let mut html = Markup::new()
        .markup("<section><h2 title=\"Retention policy\">How long data is kept</h2><ul>");
    for (machine, days) in &state.retention {
        let policy = days.map_or_else(
            || "kept, never deleted".into(),
            |days| {
                format!(
                    "deleted {} after the lab app last confirmed it",
                    format::amount(days, "day", "days")
                )
            },
        );
        html.push_fragment(
            &Markup::new()
                .markup("<li>")
                .text(machine.as_str())
                .markup(": ")
                .text(&policy)
                .markup("</li>"),
        );
    }
    if state.retention.is_empty() {
        html.push_markup("<li>No machine is set up.</li>");
    }
    html.push_markup("</ul></section><section><h2 title=\"Last purge run\">Last clean-up</h2>");
    html.push_fragment(&last.map_or_else(
        || Markup::new().markup("<p class=\"muted\">No clean-up has run yet.</p>"),
        |run| last_run(&run, now),
    ));
    html.push_markup(
        "</section><section><h2 title=\"Oldest retained receipt\">Oldest message kept</h2><p>",
    );
    html.push_fragment(&oldest.map_or_else(
        || Markup::new().text("None kept."),
        |time| {
            Markup::new()
                .text("Received ")
                .fragment(&format::when_unix(time, now))
        },
    ));
    html.push_markup(concat!(
        "</p></section><section><h2 title=\"Active holds\">Holds</h2><p class=\"muted\">",
        "A hold keeps data from clean-up until someone releases it; it never expires. ",
        "Holding an order also keeps its messages. When Analyte is not sure what belongs ",
        "together, it keeps more.</p><ul>",
    ));
    let next = holds.last().map(|hold| hold.sequence);
    if holds.is_empty() {
        html.push_markup("<li>No holds.</li>");
    }
    for hold in holds {
        html.push_fragment(
            &Markup::new()
                .markup("<li title=\"")
                .text(&serde_json::to_string(&hold.target)?)
                .markup("\">")
                .fragment(&held(&hold.target))
                .markup("</li>"),
        );
    }
    html.push_markup("</ul>");
    if let Some(after) = next {
        html.push_fragment(
            &Markup::new()
                .markup("<p><a class=\"button secondary\" rel=\"next\" href=\"/retention?after=")
                .text(after)
                .markup("\">More holds</a></p>"),
        );
    }
    html.push_markup("</section>");
    render_form(&mut html, session);
    Ok(html)
}
/// What the last clean-up did, as plain numbers; failures show red.
fn last_run(run: &analyte_store::PurgeRun, now: jiff::Timestamp) -> Markup {
    let number = |value: &dyn std::fmt::Display| Markup::new().text(value);
    Markup::new()
        .markup("<dl>")
        .fragment(&view::row("Ran", None, &format::when_unix(run.time, now)))
        .fragment(&view::row(
            "Messages checked",
            Some("Examined"),
            &number(&run.examined),
        ))
        .fragment(&view::row(
            "Messages deleted",
            Some("Purged receipts"),
            &number(&run.purged),
        ))
        .fragment(&view::row(
            "Orders deleted",
            Some("Purged orders"),
            &number(&run.orders),
        ))
        .fragment(&view::row("Paused", None, &number(&run.paused)))
        .fragment(&view::row("Failed", None, &count(u64::from(run.failed))))
        .markup("</dl>")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct HoldForm {
    csrf: String,
    scope: String,
    id: String,
    destination: String,
    operation: String,
    reason: String,
    confirm: String,
}
pub(in crate::admin) async fn hold(
    State(state): State<Arc<Context>>,
    Extension(session): Extension<Arc<Session>>,
    Form(form): Form<HoldForm>,
) -> Response {
    if !session.authenticated || !session.csrf_valid(&form.csrf) || form.confirm != "yes" {
        return view::refusal(
            StatusCode::FORBIDDEN,
            "Hold not saved",
            "Tick the confirmation box and make sure you are signed in, then try again.",
            "authenticated session, valid form and confirmation required",
        );
    }
    let result = apply(&state, &session, form).await;
    match result {
        Ok(()) => page(&Markup::new().markup(concat!(
            "<p class=\"notice info\">Hold saved. Your session and reason are recorded with ",
            "it.</p><p><a href=\"/retention\">Back to Data keeping</a></p>",
        ))),
        Err(error) => failure(&error),
    }
}
async fn apply(state: &Context, session: &Session, form: HoldForm) -> Result<(), Error> {
    let target = match form.scope.as_str() {
        "receipt" => {
            HoldTarget::Receipt(serde_json::from_value(serde_json::Value::String(form.id))?)
        }
        "order" => HoldTarget::Order {
            destination: Identifier::new(&form.destination)?,
            order: Identifier::new(&form.id)?,
        },
        _ => return Err(Error::ChooseReceiptOrOrder),
    };
    let active = match form.operation.as_str() {
        "place" => true,
        "release" => false,
        _ => return Err(Error::ChoosePlaceOrRelease),
    };
    let audit = Audit {
        actor: session.actor.clone(),
        action: Identifier::new(if active {
            "retention-hold-place"
        } else {
            "retention-hold-release"
        })?,
        targets: Vec::new(),
        time: jiff::Timestamp::now().as_second(),
        reason: Reason::new(&form.reason)?,
    };
    state
        .store
        .request(deadline()?, move |store| {
            store.set_retention_hold(&target, active, &audit)
        })
        .await?;
    Ok(())
}

/// What a hold keeps, in words; the stored target stays on hover.
fn held(target: &HoldTarget) -> Markup {
    match target {
        HoldTarget::Receipt(id) => Markup::new()
            .markup("Message <code>")
            .text(uuid::Uuid::from(*id))
            .markup("</code>"),
        HoldTarget::Order { destination, order } => Markup::new()
            .markup("Order <code>")
            .text(order.as_str())
            .markup("</code> from lab app ")
            .text(destination.as_str()),
    }
}

fn render_form(html: &mut Markup, session: Option<&Session>) {
    if let Some(session) = session.filter(|s| s.authenticated) {
        html.push_fragment(
            &Markup::new()
                .markup(concat!(
                    "<section><h2>Add or release a hold</h2><form ",
                    "action=\"/retention\" method=\"post\"><input ",
                ))
                .markup("type=\"hidden\" name=\"csrf\" value=\"")
                .text(session.csrf.form_value())
                .markup(concat!(
                    "\"><label class=\"field\"><span title=\"Scope\">Hold a</span><select name=\"scope\"><option ",
                    "value=\"receipt\">Message</option><option ",
                ))
                .markup(concat!(
                    "value=\"order\">Order</option></select></label><label class=\"field\">",
                    "<span title=\"Receipt UUID or app order ID\">Message number or lab app order number</span><input ",
                ))
                .markup(concat!(
                    "name=\"id\" required ",
                    "maxlength=\"256\"></label><label class=\"field\"><span title=\"Destination\">Lab app ",
                    "(orders only)</span><input ",
                ))
                .markup(concat!(
                    "name=\"destination\" ",
                    "maxlength=\"256\"></label><label class=\"field\"><span>Action</span><select ",
                    "name=\"operation\"><option ",
                ))
                .markup(concat!(
                    "value=\"place\">Add hold</option><option ",
                    "value=\"release\">Release ",
                ))
                .markup(concat!(
                    "hold</option></select></label><label class=\"field\"><span>Reason</span>",
                    "<input name=\"reason\" required ",
                ))
                .markup(concat!(
                    "maxlength=\"256\"></label><label class=\"check\"><input ",
                    "type=\"checkbox\" name=\"confirm\" value=\"yes\" ",
                ))
                .markup(concat!(
                    "required>I confirm this hold ",
                    "change.</label><div class=\"actions-row\"><button>Save hold ",
                    "change</button></div></form></section>",
                )),
        );
    }
}
