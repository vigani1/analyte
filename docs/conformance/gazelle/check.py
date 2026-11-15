"""Submit Analyte's synthetic LAW messages to IHE Gazelle's HL7v2 validator.

Usage: python -I gazelle_check.py <messages dir> <results dir>

Only synthetic messages may be submitted. Uses the standard library only.
"""

import base64
import json
import pathlib
import re
import sys
import time
import urllib.request
from xml.sax.saxutils import escape

EVS = "https://connectathon.ihe-catalyst.net/evs/rest/validations"
SERVICE = "HL7v2 Validation service"
AGENT = "analyte-conformance-check"

# File name -> Gazelle profile OID (IHE PaLM LAW, HL7 v2.5.1).
PROFILES = {
    "rsp_k11_found.hl7": "1.3.6.1.4.12559.11.1.1.177",
    "rsp_k11_empty.hl7": "1.3.6.1.4.12559.11.1.1.177",
    "oml_o33_new.hl7": "1.3.6.1.4.12559.11.1.1.178",
    "oml_o33_cancel.hl7": "1.3.6.1.4.12559.11.1.1.178",
    "oml_o33_new_parent_carrier.hl7": "1.3.6.1.4.12559.11.1.1.178",
    "oml_o33_negative.hl7": "1.3.6.1.4.12559.11.1.1.215",
    "ack_r22_aa.hl7": "1.3.6.1.4.12559.11.1.1.179",
    "ack_r22_ae.hl7": "1.3.6.1.4.12559.11.1.1.179",
    "qbp_q11_specimen.hl7": "1.3.6.1.4.12559.11.1.1.180",
    "qbp_q11_all_pending.hl7": "1.3.6.1.4.12559.11.1.1.180",
    "orl_o34_accept.hl7": "1.3.6.1.4.12559.11.1.1.181",
    "oul_r22.hl7": "1.3.6.1.4.12559.11.1.1.182",
}


def request(url, data=None, accept=None, content_type=None):
    req = urllib.request.Request(url, data=data, method="POST" if data else "GET")
    req.add_header("User-Agent", AGENT)
    if accept:
        req.add_header("Accept", accept)
    if content_type:
        req.add_header("Content-Type", content_type)
    with urllib.request.urlopen(req, timeout=120) as resp:
        return resp.status, dict(resp.headers), resp.read()


def submit(name, raw, oid):
    body = (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<validation xmlns="http://evsobjects.gazelle.ihe.net/">'
        f'<validationService name="{escape(SERVICE)}" validator="{escape(oid)}"/>'
        f'<object originalFileName="{escape(name)}">'
        f"<content>{base64.b64encode(raw).decode()}</content></object></validation>"
    ).encode()
    status, headers, _ = request(EVS, body, content_type="application/xml")
    location = headers.get("Location") or headers.get("Content-Location")
    if status not in (201, 202) or not location:
        raise RuntimeError(f"{name}: submission returned {status}")
    return location


def report(location):
    for _ in range(30):
        try:
            status, _, data = request(
                location + "/report", accept="application/gzl.validation.report+xml"
            )
            if status == 200 and b"validationOverallResult" in data:
                return data
        except urllib.error.HTTPError:
            pass
        time.sleep(2)
    raise RuntimeError(f"no report at {location}")


def summarize(xml):
    text = xml.decode("utf-8", "replace")
    result = re.search(r'validationOverallResult="([A-Z_]+)"', text)
    counts = {"ERROR": 0, "WARNING": 0}
    for e, w in re.findall(r'numberOfErrors="(\d+)" numberOfWarnings="(\d+)"', text):
        counts["ERROR"] += int(e)
        counts["WARNING"] += int(w)
    errors = []
    for block in re.findall(r"<gvr:constraint\b.*?</gvr:constraint>", text, flags=re.S):
        if 'severity="ERROR"' in block and 'testResult="FAILED"' in block:
            desc = re.search(r"<gvr:constraintDescription>(.*?)</gvr:constraintDescription>", block, re.S)
            loc = re.search(r"<gvr:locationInValidatedObject>(.*?)</gvr:locationInValidatedObject>", block, re.S)
            errors.append(
                {
                    "location": loc.group(1).strip() if loc else "",
                    "description": re.sub(r"\s+", " ", desc.group(1)).strip() if desc else "",
                }
            )
    return result.group(1) if result else "UNKNOWN", counts, errors


def main():
    source = pathlib.Path(sys.argv[1])
    out = pathlib.Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    summary = []
    for name, oid in PROFILES.items():
        path = source / name
        if not path.exists():
            summary.append({"file": name, "result": "MISSING"})
            continue
        location = submit(name, path.read_bytes(), oid)
        xml = report(location)
        (out / (path.stem + ".report.xml")).write_bytes(xml)
        result, counts, errors = summarize(xml)
        summary.append(
            {
                "file": name,
                "profile": oid,
                "result": result,
                "errors": counts["ERROR"],
                "warnings": counts["WARNING"],
                "validation": location,
                "error_details": errors,
            }
        )
        print(f"{name}: {result} errors={counts['ERROR']} warnings={counts['WARNING']}")
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", newline="\n")


if __name__ == "__main__":
    main()
