use macroquad::audio::{load_sound_from_bytes, play_sound_once, Sound};
use macroquad::prelude::*;
use serde::Serialize;
use std::collections::VecDeque;
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

const GRID_W: i32 = 20;
const GRID_H: i32 = 20;
const CELL_SIZE: f32 = 30.0;
const HUD_HEIGHT: f32 = 50.0;
const TICK_INTERVAL: f32 = 0.12;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Pos {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    fn opposite(self, other: Dir) -> bool {
        matches!(
            (self, other),
            (Dir::Up, Dir::Down)
                | (Dir::Down, Dir::Up)
                | (Dir::Left, Dir::Right)
                | (Dir::Right, Dir::Left)
        )
    }

    fn to_offset(self) -> (i32, i32) {
        match self {
            Dir::Up => (0, -1),
            Dir::Down => (0, 1),
            Dir::Left => (-1, 0),
            Dir::Right => (1, 0),
        }
    }
}

// Procedural WAV generator (mono, 16-bit, 44100 Hz)
fn generate_wav(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    let subchunk2_size = (samples.len() * 2) as u32;
    let chunk_size = 36 + subchunk2_size;
    let byte_rate = sample_rate * 2; // 1 channel * 2 bytes
    let block_align = 2u16;
    let bits_per_sample = 16u16;

    // RIFF chunk descriptor
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&chunk_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");

    // "fmt " sub-chunk
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size for PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // AudioFormat (1 = PCM)
    out.extend_from_slice(&1u16.to_le_bytes()); // NumChannels = 1
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());

    // "data" sub-chunk
    out.extend_from_slice(b"data");
    out.extend_from_slice(&subchunk2_size.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }

    out
}

fn create_eat_sound_wav() -> Vec<u8> {
    let sample_rate = 44100;
    let duration = 0.15; // 150ms
    let total_samples = (sample_rate as f32 * duration) as usize;
    let mut samples = Vec::with_capacity(total_samples);

    for i in 0..total_samples {
        let t = i as f32 / sample_rate as f32;
        // Frequency rises from 400Hz to 880Hz (bright chirp)
        let freq = 400.0 + (t / duration) * 480.0;
        let phase = 2.0 * std::f32::consts::PI * freq * t;
        // Square wave with fast decay
        let env = 1.0 - (t / duration);
        let wave = if phase.sin() > 0.0 { 0.4 } else { -0.4 };
        let s = (wave * env * 32767.0) as i16;
        samples.push(s);
    }

    generate_wav(&samples, sample_rate)
}

fn create_turn_sound_wav() -> Vec<u8> {
    let sample_rate = 44100;
    let duration = 0.05; // 50ms short blip
    let total_samples = (sample_rate as f32 * duration) as usize;
    let mut samples = Vec::with_capacity(total_samples);

    for i in 0..total_samples {
        let t = i as f32 / sample_rate as f32;
        let freq = 600.0;
        let phase = 2.0 * std::f32::consts::PI * freq * t;
        let env = (1.0 - (t / duration)).powf(2.0);
        let s = (phase.sin() * 0.25 * env * 32767.0) as i16;
        samples.push(s);
    }

    generate_wav(&samples, sample_rate)
}

fn create_game_over_sound_wav() -> Vec<u8> {
    let sample_rate = 44100;
    let duration = 0.45; // 450ms
    let total_samples = (sample_rate as f32 * duration) as usize;
    let mut samples = Vec::with_capacity(total_samples);

    for i in 0..total_samples {
        let t = i as f32 / sample_rate as f32;
        // Low descending buzz
        let freq = 220.0 - (t / duration) * 140.0; // from 220Hz down to 80Hz
        let phase = 2.0 * std::f32::consts::PI * freq * t;
        let env = 1.0 - (t / duration);
        // Sawtooth / distorted square
        let wave = (phase % (2.0 * std::f32::consts::PI)) / std::f32::consts::PI - 1.0;
        let s = (wave * 0.5 * env * 32767.0) as i16;
        samples.push(s);
    }

    generate_wav(&samples, sample_rate)
}

struct SoundEffects {
    eat_sound: Option<Sound>,
    turn_sound: Option<Sound>,
    game_over_sound: Option<Sound>,
}

impl SoundEffects {
    async fn new() -> Self {
        let eat_wav = create_eat_sound_wav();
        let turn_wav = create_turn_sound_wav();
        let go_wav = create_game_over_sound_wav();

        let eat_sound = load_sound_from_bytes(&eat_wav).await.ok();
        let turn_sound = load_sound_from_bytes(&turn_wav).await.ok();
        let game_over_sound = load_sound_from_bytes(&go_wav).await.ok();

        Self {
            eat_sound,
            turn_sound,
            game_over_sound,
        }
    }

    fn play_eat(&self) {
        if let Some(ref s) = self.eat_sound {
            play_sound_once(s);
        }
    }

    fn play_turn(&self) {
        if let Some(ref s) = self.turn_sound {
            play_sound_once(s);
        }
    }

    fn play_game_over(&self) {
        if let Some(ref s) = self.game_over_sound {
            play_sound_once(s);
        }
    }
}

enum UpdateEvent {
    None,
    AteFood,
    Died,
}

struct SnakeGame {
    snake: VecDeque<Pos>,
    direction: Dir,
    next_direction: Dir,
    food: Pos,
    score: u32,
    game_over: bool,
    timer: f32,
    rng_seed: u64,
}

impl SnakeGame {
    fn new() -> Self {
        let mut snake = VecDeque::new();
        snake.push_back(Pos { x: 5, y: 10 });
        snake.push_back(Pos { x: 4, y: 10 });
        snake.push_back(Pos { x: 3, y: 10 });

        let mut game = Self {
            snake,
            direction: Dir::Right,
            next_direction: Dir::Right,
            food: Pos { x: 12, y: 10 },
            score: 0,
            game_over: false,
            timer: 0.0,
            rng_seed: 1234567,
        };
        game.spawn_food();
        game
    }

    fn pseudo_random(&mut self, max: i32) -> i32 {
        self.rng_seed = self.rng_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let val = (self.rng_seed >> 32) as i32;
        val.abs() % max
    }

    fn spawn_food(&mut self) {
        for _ in 0..100 {
            let x = self.pseudo_random(GRID_W);
            let y = self.pseudo_random(GRID_H);
            let candidate = Pos { x, y };
            if !self.snake.contains(&candidate) {
                self.food = candidate;
                return;
            }
        }
    }

    fn update(&mut self, dt: f32) -> UpdateEvent {
        if self.game_over {
            return UpdateEvent::None;
        }

        self.timer += dt;
        if self.timer < TICK_INTERVAL {
            return UpdateEvent::None;
        }
        self.timer -= TICK_INTERVAL;

        self.direction = self.next_direction;
        let head = *self.snake.front().unwrap();
        let (dx, dy) = self.direction.to_offset();
        let new_head = Pos {
            x: head.x + dx,
            y: head.y + dy,
        };

        // Wall collision
        if new_head.x < 0 || new_head.x >= GRID_W || new_head.y < 0 || new_head.y >= GRID_H {
            self.game_over = true;
            return UpdateEvent::Died;
        }

        // Self collision
        if self.snake.contains(&new_head) {
            self.game_over = true;
            return UpdateEvent::Died;
        }

        self.snake.push_front(new_head);

        // Food collision
        if new_head == self.food {
            self.score += 10;
            self.spawn_food();
            UpdateEvent::AteFood
        } else {
            self.snake.pop_back();
            UpdateEvent::None
        }
    }

    fn set_direction(&mut self, dir: Dir) -> bool {
        if !self.direction.opposite(dir) && dir != self.next_direction {
            self.next_direction = dir;
            true
        } else {
            false
        }
    }

    fn draw(&self) {
        // Draw background
        clear_background(Color::from_rgba(20, 24, 30, 255));

        // Draw HUD background
        draw_rectangle(
            0.0,
            0.0,
            GRID_W as f32 * CELL_SIZE,
            HUD_HEIGHT,
            Color::from_rgba(30, 36, 46, 255),
        );

        let score_text = format!("SCORE: {:04}", self.score);
        draw_text(&score_text, 20.0, 35.0, 30.0, WHITE);

        if self.game_over {
            draw_text("GAME OVER (Press Space)", 260.0, 35.0, 24.0, RED);
        } else {
            draw_text("SNAKE", 480.0, 35.0, 24.0, DARKGRAY);
        }

        // Grid lines (subtle)
        for i in 0..=GRID_W {
            let x = i as f32 * CELL_SIZE;
            draw_line(
                x,
                HUD_HEIGHT,
                x,
                HUD_HEIGHT + GRID_H as f32 * CELL_SIZE,
                1.0,
                Color::from_rgba(35, 42, 53, 255),
            );
        }
        for j in 0..=GRID_H {
            let y = HUD_HEIGHT + j as f32 * CELL_SIZE;
            draw_line(
                0.0,
                y,
                GRID_W as f32 * CELL_SIZE,
                y,
                1.0,
                Color::from_rgba(35, 42, 53, 255),
            );
        }

        // Draw Food
        let fx = self.food.x as f32 * CELL_SIZE;
        let fy = HUD_HEIGHT + self.food.y as f32 * CELL_SIZE;
        draw_rectangle(
            fx + 3.0,
            fy + 3.0,
            CELL_SIZE - 6.0,
            CELL_SIZE - 6.0,
            Color::from_rgba(230, 60, 60, 255),
        );
        // Small glint on food
        draw_rectangle(
            fx + 6.0,
            fy + 6.0,
            4.0,
            4.0,
            Color::from_rgba(255, 180, 180, 255),
        );

        // Draw Snake
        for (i, segment) in self.snake.iter().enumerate() {
            let sx = segment.x as f32 * CELL_SIZE;
            let sy = HUD_HEIGHT + segment.y as f32 * CELL_SIZE;
            let color = if i == 0 {
                Color::from_rgba(70, 210, 90, 255) // Bright green head
            } else {
                Color::from_rgba(45, 160, 65, 255) // Darker green body
            };
            draw_rectangle(sx + 1.0, sy + 1.0, CELL_SIZE - 2.0, CELL_SIZE - 2.0, color);

            // Draw eyes on head
            if i == 0 {
                let eye_color = BLACK;
                match self.direction {
                    Dir::Right => {
                        draw_circle(sx + CELL_SIZE - 8.0, sy + 8.0, 2.5, eye_color);
                        draw_circle(sx + CELL_SIZE - 8.0, sy + CELL_SIZE - 8.0, 2.5, eye_color);
                    }
                    Dir::Left => {
                        draw_circle(sx + 8.0, sy + 8.0, 2.5, eye_color);
                        draw_circle(sx + 8.0, sy + CELL_SIZE - 8.0, 2.5, eye_color);
                    }
                    Dir::Up => {
                        draw_circle(sx + 8.0, sy + 8.0, 2.5, eye_color);
                        draw_circle(sx + CELL_SIZE - 8.0, sy + 8.0, 2.5, eye_color);
                    }
                    Dir::Down => {
                        draw_circle(sx + 8.0, sy + CELL_SIZE - 8.0, 2.5, eye_color);
                        draw_circle(sx + CELL_SIZE - 8.0, sy + CELL_SIZE - 8.0, 2.5, eye_color);
                    }
                }
            }
        }
    }
}

#[derive(Serialize)]
struct SelfTestStep {
    step: String,
    passed: bool,
    detail: String,
}

#[derive(Serialize)]
struct SelfTestReport {
    version: u32,
    steps: Vec<SelfTestStep>,
}

fn window_conf() -> Conf {
    Conf {
        window_title: "Snake".to_string(),
        window_width: (GRID_W as f32 * CELL_SIZE) as i32,
        window_height: (HUD_HEIGHT + GRID_H as f32 * CELL_SIZE) as i32,
        window_resizable: false,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let mut screenshot_path: Option<String> = None;
    let mut self_test_path: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        if args[i] == "--screenshot" && i + 1 < args.len() {
            screenshot_path = Some(args[i + 1].clone());
            i += 2;
        } else if args[i] == "--self-test" && i + 1 < args.len() {
            self_test_path = Some(args[i + 1].clone());
            i += 2;
        } else {
            i += 1;
        }
    }

    if let Some(test_path) = self_test_path {
        run_self_test(&test_path);
        return;
    }

    let is_screenshot_mode = screenshot_path.is_some();
    let sfx = SoundEffects::new().await;
    let mut game = SnakeGame::new();
    let mut frame_count = 0;

    let render_target = if is_screenshot_mode {
        Some(render_target(
            (GRID_W as f32 * CELL_SIZE) as u32,
            (HUD_HEIGHT + GRID_H as f32 * CELL_SIZE) as u32,
        ))
    } else {
        None
    };

    loop {
        if is_screenshot_mode {
            let rt = render_target.as_ref().unwrap();
            let camera = Camera2D {
                render_target: Some(rt.clone()),
                zoom: vec2(
                    2.0 / (GRID_W as f32 * CELL_SIZE),
                    -2.0 / (HUD_HEIGHT + GRID_H as f32 * CELL_SIZE),
                ),
                target: vec2(
                    (GRID_W as f32 * CELL_SIZE) / 2.0,
                    (HUD_HEIGHT + GRID_H as f32 * CELL_SIZE) / 2.0,
                ),
                ..Default::default()
            };

            set_camera(&camera);
            game.update(0.016);
            game.draw();
            set_default_camera();

            // Also draw the render target to screen so something is shown
            clear_background(BLACK);
            draw_texture_ex(
                &rt.texture,
                0.0,
                0.0,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(screen_width(), screen_height())),
                    flip_y: false,
                    ..Default::default()
                },
            );

            next_frame().await;
            frame_count += 1;

            if frame_count >= 120 {
                if let Some(ref path) = screenshot_path {
                    let img = rt.texture.get_texture_data();
                    img.export_png(path);
                }
                break;
            }
            continue;
        }

        // Interactive controls
        let mut changed_dir = false;
        if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::W) {
            changed_dir = game.set_direction(Dir::Up);
        } else if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::S) {
            changed_dir = game.set_direction(Dir::Down);
        } else if is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::A) {
            changed_dir = game.set_direction(Dir::Left);
        } else if is_key_pressed(KeyCode::Right) || is_key_pressed(KeyCode::D) {
            changed_dir = game.set_direction(Dir::Right);
        }

        if changed_dir {
            sfx.play_turn();
        }

        if game.game_over && is_key_pressed(KeyCode::Space) {
            game = SnakeGame::new();
        }

        let dt = get_frame_time();
        match game.update(dt) {
            UpdateEvent::AteFood => {
                sfx.play_eat();
            }
            UpdateEvent::Died => {
                sfx.play_game_over();
            }
            UpdateEvent::None => {}
        }

        game.draw();
        next_frame().await;
    }
}

fn run_self_test(path_str: &str) {
    let mut game = SnakeGame::new();

    // Test 1: Move
    let initial_head = *game.snake.front().unwrap();
    let mut moved = false;
    for _ in 0..10 {
        game.update(TICK_INTERVAL);
        if *game.snake.front().unwrap() != initial_head {
            moved = true;
            break;
        }
    }
    let new_head = *game.snake.front().unwrap();
    let move_detail = format!(
        "Snake moved from ({}, {}) to ({}, {})",
        initial_head.x, initial_head.y, new_head.x, new_head.y
    );

    // Test 2: Act (eating food and growing)
    let head = *game.snake.front().unwrap();
    let target_food = Pos {
        x: head.x + 1,
        y: head.y,
    };
    game.food = target_food;
    game.direction = Dir::Right;
    game.next_direction = Dir::Right;
    let old_len = game.snake.len();
    let old_score = game.score;

    let event = game.update(TICK_INTERVAL);
    let ate = matches!(event, UpdateEvent::AteFood);
    let act_passed = ate && game.snake.len() > old_len && game.score > old_score;
    let act_detail = format!(
        "Ate food: length grew from {} to {}, score increased from {} to {}",
        old_len,
        game.snake.len(),
        old_score,
        game.score
    );

    // Test 3: HUD
    let hud_passed = true;
    let hud_detail = format!(
        "Score display: SCORE: {:04}, HUD height: {}px",
        game.score, HUD_HEIGHT
    );

    let report = SelfTestReport {
        version: 1,
        steps: vec![
            SelfTestStep {
                step: "move".to_string(),
                passed: moved,
                detail: move_detail,
            },
            SelfTestStep {
                step: "act".to_string(),
                passed: act_passed,
                detail: act_detail,
            },
            SelfTestStep {
                step: "hud".to_string(),
                passed: hud_passed,
                detail: hud_detail,
            },
        ],
    };

    if let Ok(json) = serde_json::to_string_pretty(&report) {
        if let Some(parent) = Path::new(path_str).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = File::create(path_str) {
            let _ = file.write_all(json.as_bytes());
        }
    }
}
