# Source baselines

Read-only Git queries and source inspection on 2026-11-20 gave these pins. Source references name the repository and short revision, for example `effectcraft@f32ad3aa:Cargo.toml:128`.

| Repository | HEAD | Branch and state | Use |
|---|---|---|---|
| effectcraft | f32ad3aa3b0a8e853ad41db3965fe19455a693cb | Detached, clean | EffectCraft design target |
| photocraft | 359051359fee33177349d93fd12ac44af9ae4e1a | Detached, clean | PhotoCraft design target |
| craft-libs | e6dbc97ca3744931fe58235d5eff126b2b696098 | Detached, clean | Framework design target |
| craft-ui-port-kit | 27e5ff34b387da7139878bf44247b4ef11497cf6 | main, clean | Framework port rules |
| craft-extensions | 223e52fe2a9658390dca6c33b5588e7df38c7d9f | plugin-beta, modified/staged/untracked | Historical implementation, use HEAD for citations |

The beta has active changes to runtime, GPU, compatibility, author tooling, templates and build records. Source observations about uncommitted files require an explicit worktree label. Historical beta docs have no independent runtime-verification status.

Owner addendum controls. First apps are EffectCraft and PhotoCraft. FigCraft follows. The older store, network, Adobe hosts, commit/push/build permissions and model selection are superseded for this design. Preserve canonical app engine and framework code. Replace legacy plugin mechanisms only with data-preserving migration and matched workflow evidence.

## Framework dependency caveat

EffectCraft pins `craft-ui` at `5a7a63a21dd03dd1a2bc6c308a3a6ce818caae87` in `effectcraft@f32ad3aa:Cargo.toml:128`. The commit is present in the local framework object database. PhotoCraft pins the `craft` facade at `bafb4522899d9f814a07dad636cdefcbf705dead` in `photocraft@359051359:Cargo.toml:15` and the egui fork at `e9711ca9b69772f875ae0d1cef1977ff8c16234d` at lines 346–359. The facade pin is absent from the two local craft-libs object databases but its public raw source and tree are readable. The local main tree at `e6dbc97ca` lacks the split facade/behavior/ui-patterns packages. The locally available `upstream/next` at `427c816db0ac8c72f3ba052086f642ed1be398fe` has them and is a different pin. Use PhotoCraft's exact resolved revision for framework evidence. The integration plan must establish a shared framework source and one egui family before EffectCraft mounts through the new framework. [PhotoCraft's pinned framework manifest](https://raw.githubusercontent.com/storytold/craft-libs/bafb4522899d9f814a07dad636cdefcbf705dead/Cargo.toml)
