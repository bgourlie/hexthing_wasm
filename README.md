This repo contains a simple hex grid renderer written in rust targeting web assembly. It also integrates the specs entity component system (the first such example that exists targeting web assembly as far as I'm aware) but is unable to do rendering within a render loop due to certain constraints outline in [this stackoverflow post](https://stackoverflow.com/questions/53000413/how-can-i-work-around-not-being-able-to-export-functions-with-lifetimes-when-usi).

### Prerequisites
- Works with the most recent stable version of rust (1.30.0 as of the time of this writing)
- You will need to install `wasm-pack`: `cargo install wasm-pack`

### Running

- `git clone https://github.com/bgourlie/hexthing_wasm.git`
- `cd hexthing_wasm`
- `npm install`
- `cd crate`
- `wasm-pack build`
- `cd ../`
- `npm run start`

### Notes

Specs is currently brought in as a git dependency referencing a specific revision, since changes required to target web-assembly have yet to be released.
