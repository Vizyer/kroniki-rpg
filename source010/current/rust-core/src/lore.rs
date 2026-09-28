use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoreFact {
    pub id: String,
    pub text: String,
    pub tags: Vec<String>,
    pub canon: bool,
    pub year: Option<i32>,
    pub month: Option<u8>,
    pub day: Option<u8>,
    pub scope: String,
    pub confidence: i32,
    pub source_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoreQuery {
    pub text: String,
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub character_only: bool,
    pub known_fact_ids: Vec<String>,
    pub limit: usize,
}

pub fn search<'a>(facts: &'a [LoreFact], q: &LoreQuery) -> Vec<&'a LoreFact> {
    let terms: Vec<String> = q.text
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|x| x.len() > 2)
        .map(str::to_string)
        .collect();

    let mut scored: Vec<(i32, &LoreFact)> = facts.iter()
        .filter(|f| is_available_by_date(f, q))
        .filter(|f| !q.character_only || f.scope == "common" || q.known_fact_ids.contains(&f.id))
        .map(|f| {
            let hay = format!("{} {}", f.text.to_lowercase(), f.tags.join(" ").to_lowercase());
            let score = terms.iter().map(|t| if hay.contains(t) { 3 } else { 0 }).sum::<i32>()
                + if f.canon { 1 } else { 0 }
                + f.confidence / 40;
            (score, f)
        })
        .filter(|(score, _)| *score > 0)
        .collect();

    scored.sort_by(|a,b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
    scored.into_iter().take(q.limit.clamp(1, 12)).map(|(_,f)|f).collect()
}

fn is_available_by_date(f: &LoreFact, q: &LoreQuery) -> bool {
    let Some(y) = f.year else { return true; };
    if y < q.year { return true; }
    if y > q.year { return false; }
    let fm = f.month.unwrap_or(1);
    if fm < q.month { return true; }
    if fm > q.month { return false; }
    f.day.unwrap_or(1) <= q.day
}

pub fn starter_facts() -> Vec<LoreFact> {
    vec![
        LoreFact {
            id:"witcher_mutations".into(),
            text:"Wiedźmini przechodzą mutacje i specjalistyczne szkolenie do walki z potworami.".into(),
            tags:vec!["wiedźmin".into(),"mutacje".into(),"potwory".into()],
            canon:true, year:None, month:None, day:None,
            scope:"common".into(), confidence:100, source_label:"canon".into()
        },
        LoreFact {
            id:"chaos_magic".into(),
            text:"Magia wykorzystuje Chaos i wymaga kontroli; silna ingerencja może mieć koszt i konsekwencje.".into(),
            tags:vec!["magia".into(),"chaos".into(),"czarodziej".into()],
            canon:true, year:None, month:None, day:None,
            scope:"common".into(), confidence:100, source_label:"canon".into()
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn future_fact_does_not_leak() {
        let facts=vec![LoreFact{id:"future".into(),text:"Przyszłe wydarzenie".into(),tags:vec!["wydarzenie".into()],canon:true,year:Some(1300),month:Some(1),day:Some(1),scope:"common".into(),confidence:100,source_label:"test".into()}];
        let q=LoreQuery{text:"wydarzenie".into(),year:1272,month:1,day:1,character_only:false,known_fact_ids:vec![],limit:5};
        assert!(search(&facts,&q).is_empty());
    }
}
