use crate::domain::Character;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateCharacterRequest {
    pub mode: String,
    pub name: Option<String>,
    pub concept: Option<String>,
    pub origin: Option<String>,
    pub profession: Option<String>,
}

pub fn create(req: &CreateCharacterRequest) -> Character {
    let concept = req.concept.clone().unwrap_or_default().to_lowercase();
    let mut c = Character::default();
    c.name = req.name.clone().filter(|x|!x.trim().is_empty()).unwrap_or_else(|| if req.mode=="random"{"Varen".into()}else{"Nowa postać".into()});
    c.origin = req.origin.clone().unwrap_or_else(|| infer_origin(&concept));
    c.profession = req.profession.clone().unwrap_or_else(|| infer_profession(&concept));

    let mut skills=BTreeMap::new();
    match c.profession.as_str() {
        "Mag" => { skills.insert("magic".into(),3); skills.insert("investigation".into(),2); c.vigor=18; },
        "Wiedźmin" => { skills.insert("combat".into(),3); skills.insert("alchemy".into(),2); skills.insert("investigation".into(),2); c.stamina=24; },
        "Łowca" => { skills.insert("combat".into(),2); skills.insert("investigation".into(),3); skills.insert("crafting".into(),1); },
        "Medyk" => { skills.insert("alchemy".into(),2); skills.insert("investigation".into(),2); },
        _ => { skills.insert("social".into(),1); skills.insert("investigation".into(),1); }
    }
    c.skills=skills;
    if concept.contains("charyzmat") || concept.contains("dwor") { c.attributes.insert("CHA".into(),4); }
    if concept.contains("siln") { c.attributes.insert("STR".into(),4); }
    if concept.contains("sprytn") || concept.contains("zwin") { c.attributes.insert("DEX".into(),4); }
    if concept.contains("uczony") || concept.contains("mag") { c.attributes.insert("INT".into(),4); }
    c
}

fn infer_origin(c:&str)->String{
    if c.contains("nilfgaard"){ "Były zwiadowca Nilfgaardu".into() }
    else if c.contains("thanedd") || c.contains("mag"){ "Mag po Thanedd".into() }
    else if c.contains("novigrad"){ "Novigrad".into() }
    else if c.contains("wiedźmin"){ "Szkoła wiedźmińska".into() }
    else { "Człowiek drogi".into() }
}

fn infer_profession(c:&str)->String{
    if c.contains("wiedźmin"){ "Wiedźmin".into() }
    else if c.contains("mag") || c.contains("czarodziej"){ "Mag".into() }
    else if c.contains("medyk"){ "Medyk".into() }
    else if c.contains("łow") || c.contains("tropic"){ "Łowca".into() }
    else { "Poszukiwacz".into() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concept_creates_mage_without_followup_questions(){
        let c=create(&CreateCharacterRequest{mode:"ai".into(),concept:Some("wygnany mag po Thanedd".into()),..Default::default()});
        assert_eq!(c.profession,"Mag");
        assert!(c.skills.get("magic").copied().unwrap_or(0)>=3);
    }
}
