use satelliteinc_core::content::GameContent;

pub fn load_content() -> GameContent {
    let json = include_str!("../../../../content/game.json");
    GameContent::from_json(json).expect("valid content")
}
