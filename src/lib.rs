use serde::Serialize;
use std::f32::consts::PI;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;

const MAX_FOOD: usize = 200;
const MAX_AGENTS: usize = 300;
const FOOD_ENERGY: f32 = 40.0;
// Per-food lifetime in ticks. Carcasses rot faster than grown food.
const FOOD_TTL: u32 = 400;
const CARCASS_TTL: u32 = 150;

// Agent energy economy
const AGENT_START_ENERGY: f32 = 100.0;
const METABOLISM_BASE: f32 = 0.008; // base energy burn per tick for speed=1.0, size=1.0, sense=0
const REPRO_THRESHOLD_BASE: f32 = 120.0;

// Heatmap: 80×60 grid (one cell = 10×10 world pixels). Max value 50, decays 1/tick.
const HEAT_W: usize = 80;
const HEAT_H: usize = 60;
type Heatmap = [f32; HEAT_W * HEAT_H];

#[derive(Serialize, Clone, PartialEq)]
pub enum FoodType {
    Normal,
    HighEnergy,
    Poisonous,
    Carcass,
}

#[derive(Serialize, Clone)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Serialize, Clone)]
pub struct Agent {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub heading: f32,
    // genes
    pub speed: f32,
    pub size: f32,
    pub sense: f32,
    pub energy: f32,
    pub species: u8,
    pub repro_threshold: f32,
    pub metabolism_eff: f32,
    pub age: u32,
    pub lineage: u32,
    pub generation: u32,
}

#[derive(Serialize, Clone, Copy)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Serialize, Clone)]
pub struct Food {
    pub pos: Point,
    pub f_type: FoodType,
    pub ttl: u32,
}

#[derive(Serialize, Clone)]
pub struct Grassland {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub regrow_timer: u32,
}

pub struct World {
    width: f32,
    height: f32,
    agents: Vec<Agent>,
    food: Vec<Food>,
    walls: Vec<Rect>,
    grasslands: Vec<Grassland>,
    time: u32,
    rng: u32,
    next_id: u32,
    heatmap: Heatmap,
    // Population tracking
    total_births: u32,
    total_deaths: u32,
    // Seasonal cycle: 0-999=spring, 1000-1999=summer, 2000-2999=autumn, 3000-3999=winter
    season: u32,
}

impl World {
    /// xorshift32 -> [0, 1)
    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn mutate_stress(&mut self, v: f32, lo: f32, hi: f32, stress: f32) -> f32 {
        // stress in [0, 1]; 0=normal mutation, 1=chaotic mutation
        let range = 0.2 + stress * 0.8;
        (v * (1.0 + (self.rand() - 0.5) * range * 2.0)).clamp(lo, hi)
    }

    /// Fade heatmap toward zero.
    fn decay_heatmap(&mut self) {
        for v in &mut self.heatmap {
            if *v > 0.0 {
                *v -= 1.0;
                if *v < 0.0 {
                    *v = 0.0;
                }
            }
        }
    }

    /// Record a birth or death at world coordinates (x, y).
    fn record_heatmap(&mut self, x: f32, y: f32, _is_death: bool) {
        let ci = (x / 10.0).floor() as usize;
        let cj = (y / 10.0).floor() as usize;
        if ci < HEAT_W && cj < HEAT_H {
            let idx = cj * HEAT_W + ci;
            self.heatmap[idx] = (self.heatmap[idx] + 1.0).min(50.0);
        }
    }
}

static WORLD: Mutex<Option<World>> = Mutex::new(None);

fn with_world<R>(f: impl FnOnce(&mut World) -> R) -> Option<R> {
    WORLD.lock().unwrap().as_mut().map(f)
}

#[wasm_bindgen]
pub fn init_world(width: f32, height: f32, seed: u32) {
    *WORLD.lock().unwrap() = Some(World {
        width,
        height,
        agents: Vec::new(),
        food: Vec::new(),
        walls: vec![
            Rect { x: 150.0, y: 80.0, w: 40.0, h: 200.0 },
            Rect { x: 280.0, y: 180.0, w: 60.0, h: 40.0 },
            Rect { x: 80.0, y: 220.0, w: 50.0, h: 50.0 },
        ],
        grasslands: vec![
            Grassland { x: 100.0, y: 100.0, radius: 100.0, regrow_timer: 0 },
            Grassland { x: 300.0, y: 200.0, radius: 100.0, regrow_timer: 0 },
            Grassland { x: 150.0, y: 250.0, radius: 80.0, regrow_timer: 0 },
        ],
        time: 0,
        rng: seed.max(1),
        next_id: 1,
        heatmap: [0.0; HEAT_W * HEAT_H],
        total_births: 0,
        total_deaths: 0,
        season: 0,
    });
    // Seed grasslands with initial food.
    with_world(|w| {
        let grasslands: Vec<(f32, f32, f32)> = w.grasslands.iter().map(|g| (g.x, g.y, g.radius)).collect();
        for (gx, gy, gr) in grasslands {
            for _ in 0..5 {
                let p = Point {
                    x: gx + (w.rand() - 0.5) * (gr * 0.8),
                    y: gy + (w.rand() - 0.5) * (gr * 0.8),
                };
                w.food.push(Food { pos: p, f_type: FoodType::Normal, ttl: FOOD_TTL });
            }
        }
        // Scatter some random food.
        for _ in 0..20 {
            let p = Point { x: w.rand() * w.width, y: w.rand() * w.height };
            w.food.push(Food { pos: p, f_type: FoodType::Normal, ttl: FOOD_TTL });
        }
    });
}

#[wasm_bindgen]
pub fn spawn_agent(x: f32, y: f32) {
    with_world(|w| {
        let agent = Agent {
            id: w.next_id,
            x,
            y,
            heading: w.rand() * 2.0 * PI,
            speed: 1.0 + w.rand(),
            size: 2.0 + w.rand() * 2.0,
            sense: 40.0 + w.rand() * 40.0,
            energy: AGENT_START_ENERGY,
            species: if w.rand() > 0.9 { 1 } else { 0 },
            repro_threshold: REPRO_THRESHOLD_BASE + w.rand() * 50.0,
            metabolism_eff: 0.8 + w.rand() * 0.4,
            age: 0,
            lineage: w.next_id,
            generation: 0,
        };
        w.next_id += 1;
        w.agents.push(agent);
    });
}

#[wasm_bindgen]
pub fn spawn_food() {
    with_world(|w| {
        if w.food.len() < MAX_FOOD {
            let f_type = match w.rand() {
                r if r < 0.1 => FoodType::Poisonous,
                r if r < 0.3 => FoodType::HighEnergy,
                _ => FoodType::Normal,
            };
            let p = Point { x: w.rand() * w.width, y: w.rand() * w.height };
            w.food.push(Food { pos: p, f_type, ttl: FOOD_TTL });
        }
    });
}

#[wasm_bindgen]
pub fn step() {
    with_world(|w| {
        // Advance time and track season at the top of each tick.
        let old_time = w.time;
        w.time = (old_time + 1) % 4000;
        w.season = w.time / 1000;
        // Seasonal metabolism multiplier.
        let season_metab = match w.season {
            0 => 1.0,   // spring: normal
            1 => 1.3,   // summer: hotter, burn more
            2 => 1.2,   // autumn: moderate
            _ => 1.8,   // winter: harsh
        };

        // Decay existing food; pruned before agents act so new food isn't touched this tick.
        for f in &mut w.food {
            if f.ttl > 0 {
                f.ttl -= 1;
            }
        }
        w.food.retain(|f| f.ttl > 0);

        // Fade heatmap toward zero.
        w.decay_heatmap();

        let n = w.agents.len();
        let mut babies = Vec::new();

        for i in 0..n {
            let mut a = w.agents[i].clone();
            a.age += 1;
            
            // Metabolism: fast, big, far-sighted agents burn more.
            let night = (w.time % 2000) > 1000;
            let current_sense = if night { a.sense * 0.5 } else { a.sense };
            // metabolism_eff *is* the efficiency of energy use:
            // high eff = burns more per tick but eats more effectively
            let base_cost = METABOLISM_BASE * a.speed * a.speed * a.size + 0.001 * current_sense;
            a.energy -= base_cost * a.metabolism_eff * season_metab;
            
            if a.species == 0 {
                // Prey: Find nearest food within sense range.
                let mut best: Option<(usize, f32)> = None;
                for (j, f) in w.food.iter().enumerate() {
                    let d2 = (f.pos.x - a.x).powi(2) + (f.pos.y - a.y).powi(2);
                    if d2 < current_sense * current_sense && best.is_none_or(|(_, b)| d2 < b) {
                        best = Some((j, d2));
                    }
                }

                match best {
                    Some((j, d2)) if d2.sqrt() <= a.size + 3.0 => {
                        let f = &w.food[j];
                        match f.f_type {
                            FoodType::Normal => a.energy += FOOD_ENERGY * a.metabolism_eff,
                            FoodType::HighEnergy => a.energy += FOOD_ENERGY * 2.0 * a.metabolism_eff,
                            FoodType::Poisonous => a.energy -= FOOD_ENERGY * 1.5 / a.metabolism_eff,
                            FoodType::Carcass => a.energy += FOOD_ENERGY * 2.5 * a.metabolism_eff,
                        }
                        w.food.swap_remove(j);
                    }
                    Some((j, _)) => {
                        let f = &w.food[j];
                        a.heading = (f.pos.y - a.y).atan2(f.pos.x - a.x);
                    }
                    None => {
                        // Bias toward nearest grassland instead of pure random walk.
                        let mut nearest_g: Option<(f32, f32, f32)> = None; // (dist, x, y)
                        for g in &w.grasslands {
                            let d2 = (g.x - a.x).powi(2) + (g.y - a.y).powi(2);
                            if nearest_g.is_none_or(|(bd, _, _)| d2 < bd) {
                                nearest_g = Some((d2, g.x, g.y));
                            }
                        }
                        if let Some((_d2, gx, gy)) = nearest_g {
                            a.heading = (gy - a.y).atan2(gx - a.x);
                        } else {
                            a.heading += (w.rand() - 0.5) * 0.6;
                        }
                    }
                }
            } else {
                // Predator: Find nearest prey (species 0) within sense range.
                let mut best_prey: Option<(usize, f32)> = None;
                for (j, other) in w.agents.iter().enumerate() {
                    if other.species == 0 && other.energy > 0.0 {
                        let d2 = (other.x - a.x).powi(2) + (other.y - a.y).powi(2);
                        if d2 < current_sense * current_sense && best_prey.is_none_or(|(_, b)| d2 < b) {
                            best_prey = Some((j, d2));
                        }
                    }
                }
                
                match best_prey {
                    Some((j, d2)) if d2.sqrt() <= a.size + 5.0 => {
                        w.agents[j].energy = 0.0;
                        a.energy += FOOD_ENERGY * 1.5;
                    }
                    Some((j, _)) => {
                        let prey = &w.agents[j];
                        a.heading = (prey.y - a.y).atan2(prey.x - a.x);
                    }
                    None => {
                        // Fallback: no prey nearby, scavenge the nearest carcass.
                        let mut carcass: Option<(usize, f32)> = None;
                        for (j, f) in w.food.iter().enumerate() {
                            if f.f_type == FoodType::Carcass {
                                let d2 = (f.pos.x - a.x).powi(2) + (f.pos.y - a.y).powi(2);
                                if d2 < current_sense * current_sense && carcass.is_none_or(|(_, b)| d2 < b) {
                                    carcass = Some((j, d2));
                                }
                            }
                        }
                        match carcass {
                            Some((j, d2)) if d2.sqrt() <= a.size + 3.0 => {
                                a.energy += FOOD_ENERGY * 2.5;
                                w.food.swap_remove(j);
                            }
                            Some((j, _)) => {
                                let c = &w.food[j];
                                a.heading = (c.pos.y - a.y).atan2(c.pos.x - a.x);
                            }
                            None => a.heading += (w.rand() - 0.5) * 0.6,
                        }
                    }
                }
            }
            
            a.x += a.heading.cos() * a.speed;
            a.y += a.heading.sin() * a.speed;
            
            // Wall Collision and Avoidance
            for wall in &w.walls {
                // If inside wall, push out
                if a.x > wall.x && a.x < wall.x + wall.w && a.y > wall.y && a.y < wall.y + wall.h {
                    let dx1 = a.x - wall.x;
                    let dx2 = (wall.x + wall.w) - a.x;
                    let dy1 = a.y - wall.y;
                    let dy2 = (wall.y + wall.h) - a.y;
                    let min_dist = dx1.min(dx2).min(dy1).min(dy2);
                    
                    if min_dist == dx1 { a.x -= dx1; }
                    else if min_dist == dx2 { a.x += dx2; }
                    else if min_dist == dy1 { a.y -= dy1; }
                    else { a.y += dy2; }
                }
                
                // If about to enter wall, rotate heading
                let next_x = a.x + a.heading.cos() * a.speed;
                let next_y = a.y + a.heading.sin() * a.speed;
                if next_x > wall.x && next_x < wall.x + wall.w && next_y > wall.y && next_y < wall.y + wall.h {
                    a.heading += 0.5; // Simple turn
                }
            }
            
            // Bounce off screen bounds.
            if a.x < 0.0 || a.x > w.width {
                a.heading = PI - a.heading;
                a.x = a.x.clamp(0.0, w.width);
            }
            if a.y < 0.0 || a.y > w.height {
                a.heading = -a.heading;
                a.y = a.y.clamp(0.0, w.height);
            }
            
            // Reproduce: split energy with a mutated child (parent keeps 2/3).
            if a.energy > a.repro_threshold && n + babies.len() < MAX_AGENTS {
                let child_energy = a.energy * 0.3;
                a.energy -= child_energy;
                // Stress-induced mutation: agents near death mutate more (evolutionary pressure).
                let stress = if a.energy > 0.0 { (a.repro_threshold - a.energy) / a.repro_threshold } else { 0.0 };
                let mut child = Agent {
                    id: w.next_id,
                    heading: w.rand() * 2.0 * PI,
                    speed: w.mutate_stress(a.speed, 0.3, 5.0, stress),
                    size: w.mutate_stress(a.size, 1.0, 10.0, stress),
                    sense: w.mutate_stress(a.sense, 10.0, 200.0, stress),
                    species: a.species,
                    repro_threshold: w.mutate_stress(a.repro_threshold, 80.0, 200.0, stress),
                    metabolism_eff: w.mutate_stress(a.metabolism_eff, 0.5, 1.5, stress),
                    age: 0,
                    generation: a.generation + 1,
                    ..a.clone()
                };
                child.energy = child_energy;
                w.next_id += 1;
                w.record_heatmap(child.x, child.y, false);
                w.total_births += 1;
                babies.push(child);
            }
            
            w.agents[i] = a;
        }
        
        // Index-based so a &mut borrow of grasslands is never held across w.rand()/w.food.
        for gi in 0..w.grasslands.len() {
            let resting = w.grasslands[gi].regrow_timer > 0;
            let local_radius = w.grasslands[gi].radius;
            let (gx, gy) = (w.grasslands[gi].x, w.grasslands[gi].y);
            // Count food within THIS grassland's radius
            let local_food_count = w.food.iter().filter(|f| {
                let d2 = (f.pos.x - gx).powi(2) + (f.pos.y - gy).powi(2);
                d2 < local_radius * local_radius
            }).count();
            let should_spawn = !resting && local_food_count < 10;

            if resting {
                w.grasslands[gi].regrow_timer -= 1;
            } else if should_spawn {
                // In winter, grasslands only grow 20% as often.
                let winter_penalty = if w.season == 3 { 0.2 } else { 1.0 };
                if w.rand() < winter_penalty {
                    let gr = local_radius;
                    let f_type = match w.rand() {
                        r if r < 0.15 => FoodType::HighEnergy,
                        r if r < 0.25 => FoodType::Poisonous,
                        _ => FoodType::Normal,
                    };
                    let p = Point {
                        x: gx + (w.rand() - 0.5) * (gr * 0.8),
                        y: gy + (w.rand() - 0.5) * (gr * 0.8),
                    };
                    w.food.push(Food { pos: p, f_type, ttl: FOOD_TTL });
                }
            } else {
                w.grasslands[gi].regrow_timer = 100;
            }
        }
        
        // Dead agents become carcasses, closing the food chain (prey -> food).
        let mut survivors = Vec::with_capacity(w.agents.len());
        let mut deaths = Vec::new(); // positions of dead agents
        for a in w.agents.drain(..) {
            if a.energy > 0.0 {
                survivors.push(a);
            } else {
                deaths.push((a.x, a.y));
                w.total_deaths += 1;
                if w.food.len() < MAX_FOOD {
                    w.food.push(Food { pos: Point { x: a.x, y: a.y }, f_type: FoodType::Carcass, ttl: CARCASS_TTL });
                }
            }
        }
        // Record deaths after drain completes (avoids double-mutable-borrow).
        for (dx, dy) in deaths {
            w.record_heatmap(dx, dy, true);
        }
        w.agents = survivors;
        w.agents.extend(babies);
    });
}

#[wasm_bindgen]
pub fn get_agents_state() -> String {
    with_world(|w| serde_json::to_string(&w.agents).ok())
        .flatten()
        .unwrap_or_else(|| "[]".into())
}

#[wasm_bindgen]
pub fn get_food_state() -> String {
    with_world(|w| serde_json::to_string(&w.food).ok())
        .flatten()
        .unwrap_or_else(|| "[]".into())
}

#[wasm_bindgen]
pub fn get_walls_state() -> String {
    with_world(|w| serde_json::to_string(&w.walls).ok())
        .flatten()
        .unwrap_or_else(|| "[]".into())
}

#[wasm_bindgen]
pub fn get_grasslands_state() -> String {
    with_world(|w| serde_json::to_string(&w.grasslands).ok())
        .flatten()
        .unwrap_or_else(|| "[]".into())
}

#[wasm_bindgen]
pub fn get_world_time() -> u32 {
    with_world(|w| w.time)
        .unwrap_or(0)
}

#[wasm_bindgen]
pub fn get_heatmap_state() -> Vec<f32> {
    with_world(|w| w.heatmap.to_vec())
        .unwrap_or_else(|| vec![0.0; HEAT_W * HEAT_H])
}

#[wasm_bindgen]
pub fn get_pop_stats() -> String {
    with_world(|w| {
        let season_names = ["Spring", "Summer", "Autumn", "Winter"];
        let s = season_names.get(w.season as usize).unwrap_or(&"Spring");
        format!(r#"{{"births":{}, "deaths":{}, "season":"{}"}}"#, w.total_births, w.total_deaths, s)
    })
    .unwrap_or_else(|| r#"{"births":0,"deaths":0,"season":"Spring"}"#.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_agent(energy: f32) -> Agent {
        Agent {
            id: 999, x: 10.0, y: 10.0, heading: 0.0,
            speed: 1.0, size: 2.0, sense: 50.0, energy,
            species: 0, repro_threshold: 200.0, metabolism_eff: 1.0, age: 0,
            lineage: 999, generation: 0,
        }
    }

    #[test]
    fn food_decays_over_time() {
        init_world(800.0, 600.0, 777);
        with_world(|w| w.food.push(Food {
            pos: Point { x: 5.0, y: 5.0 },
            f_type: FoodType::Normal,
            ttl: 1,
        }));
        step();
        let still_there = with_world(|w| w.food.iter().any(|f| f.pos.x == 5.0 && f.pos.y == 5.0 && f.ttl == 1)).unwrap();
        assert!(!still_there, "a ttl=1 food item should be pruned after one step");
    }

    #[test]
    fn dead_agent_drops_carcass() {
        init_world(800.0, 600.0, 12345);
        with_world(|w| w.agents.push(test_agent(0.0)));
        step();
        let carcass = with_world(|w| w.food.iter().any(|f| f.f_type == FoodType::Carcass)).unwrap();
        assert!(carcass, "a dead agent should leave a carcass");
        let all_alive = with_world(|w| w.agents.iter().all(|a| a.energy > 0.0)).unwrap();
        assert!(all_alive, "dead agents must be removed");
    }
}