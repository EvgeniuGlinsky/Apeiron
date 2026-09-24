# Verification of the research the plan rests on

**Verification date:** September 23, 2026
**Document verified:** the research "Storing messenger data on users' devices" (in Russian), dated
August 10, 2026. Not part of the repository — the author keeps it.

## Why this was done

The document was written by a language model at the project owner's request. The entire
implementation plan rests on it. Language models sometimes produce plausible references that do not
exist, so before development started, its load-bearing claims were checked independently.

**Result: the document withstands verification.** No signs of fabrication were found. One wording
is stronger than its source (see below).

---

## 1. Reproducibility of the calculations — CONFIRMED

All four scripts (`calc1.py`…`calc4.py`) were rerun on Python 3.14.4 and compared with the saved
output `расчёты\полный-вывод-расчётов.txt`.

| Metric | Result |
|---|---|
| Meaningful lines in the saved output | 492 |
| Meaningful lines in the new run | 489 |
| Lines differing in value | **0** |
| Lines that appeared only in the new run | **0** |

The three "missing" lines are the header of the saved file: a title, a sentence about
reproducibility, and `Дата прогона: 10.08.2026` (the run date). All numbers matched character for
character.

**Conclusion:** the numbers in the document were produced by code, not invented. This does not
confirm that the models themselves are correct — only that there is no forgery in the calculations.

**Note on running:** on Windows the scripts crash with `UnicodeEncodeError`, because Python writes
to the console in cp1252. This is a defect of the environment, not of the scripts. Run them like
this:

```bash
PYTHONIOENCODING=utf-8 python calc1.py
```

---

## 2. The Briar statement (§13.1) — CONFIRMED VERBATIM

The most load-bearing empirical claim of the work: both the "do not repeat their path" argument and
the battery gate of stage 4 rest on it.

The source `https://briarproject.org/news/2026-maintenance-mode/` exists. Publication date —
**July 9, 2026**. Checked:

- the stated reasons — "high battery usage and unreliable background operation on Android, missing
  features like account backup and file attachments, and a difficult user experience for adding
  contacts and communicating offline" — **match**;
- the lack of funding and the reluctance to look for it without a long-term plan — **matches**;
- "we decided that we wouldn't realistically be able to solve these issues and so we reluctantly
  decided to shut down the project", followed by a reconsideration in favor of maintenance mode —
  **matches**.

A clarification from the source that is not in the document: the reason given for the
reconsideration is that the app continued to attract new users.

---

## 3. The trilemma theorem and the bound δ ≥ 0.999999 (§7.2) — CONFIRMED

Checked against the code of `calc3.py`, section 17.

The result applied is that of Das, Meiser, Mohammadi, Kate (IEEE S&P 2018): strong anonymity is
impossible when `2·L·B < 1 − ε`, where `L` is the latency in rounds and `B` is the fraction of noise
messages. This is the headline theorem of their paper, reproduced correctly.

The lower bound on the observer's advantage: `δ ≥ 1 − f_B(L)`, where
`f_B(x) = min(1, (x + B·N·x)/(N−1))`.

Substitution for the scenario "real-time messenger without cover traffic":

```
N = 1 000 000, L = 1, B = 0
f = min(1, (1 + 0) / 999 999) = 1.000001·10⁻⁶
δ ≥ 1 − 1.000001·10⁻⁶ = 0.999999
```

The arithmetic checks out, and the form of the formula matches the structure of the primary
source's result.

**What this means in practice:** against a global passive observer (P6), the sender is identified
with near certainty. This is not "weak anonymity" but its absence. Hence the direct ban in
`threat-log.md` on wordings such as "complete anonymity".

---

## 4. Authenticity of the source PDFs — CONFIRMED BY SAMPLING

The file metadata is consistent with the claimed works:

| File | Metadata | Assessment |
|---|---|---|
| `sybil-douceur.pdf` | `Microsoft Word - IPTPS2002.doc` | consistent with Douceur, IPTPS 2002 |
| `churn-stutzbach.pdf` | `paper.dvi` | a LaTeX build, plausible |
| `ford-availability.pdf` | embedded `durations_color.eps` | consistent with a paper containing charts |

A full textual check of the quotations was not carried out: the system has no `poppler-utils`, so
text extraction from PDF is unavailable. If needed, install `poppler` or `pypdf`.

There are 24 PDFs in the folder in total; `sources.txt` describes 34 sources, stating what was taken
from each and in which section it was used — the reference apparatus is neatly structured.

---

## 5. The only discrepancy: Session and PFS (§13.2)

**The document says:** forward secrecy was "restored in December 2025 together with post-quantum
elements".

**The sources say:** in December 2025, PFS and post-quantum key exchange were **announced** as part
of the Session V2 protocol, which at the time was still being designed and required significant
resources for completion and integration.

The fact that PFS was removed in 2021, citing stability problems combined with the decentralized
architecture, is confirmed.

**Impact on the plan:** none. The argument of §13.2 (decentralization is paid for by giving up
cryptographic properties) stands and is even strengthened: the period without PFS is not "four
years" but longer, since the replacement has not shipped yet.

---

## What remains unverified

- Verbatim checking of quotations from the PDFs (a text extractor is needed).
- Stutzbach's churn figures (Weibull k = 0.34–0.79; 10–20% of nodes with uptime > one day) — they
  underpin the **rule of selecting custodians by 4 hours of continuous uptime**, the most effective
  rule of §16.5. To be checked when approaching stage 5.
- The Blake–Rodrigues limit recalculated for 2026 (§7.3) — it defines the boundary "text is
  feasible, media is not".
- The legal section (§12). The document itself states that it is not a legal opinion. A real lawyer
  is needed, especially for Ukrainian jurisdiction.

## How to re-check it yourself

```bash
cd <research directory>/расчёты
PYTHONIOENCODING=utf-8 python calc1.py
PYTHONIOENCODING=utf-8 python calc2.py
PYTHONIOENCODING=utf-8 python calc3.py
PYTHONIOENCODING=utf-8 python calc4.py
```
