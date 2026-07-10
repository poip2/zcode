---
name: skill-creator
description: MANDATORY — invoke whenever user asks to create/make/build/write/design a skill, SKILL.md, AI instruction, workflow, automation, or reusable behavior. Trigger on 帮我写skill 创建skill 加个skill 弄个skill make me a skill. Also trigger if user describes something they want AI to remember and follow in future, even without saying the word skill. NEVER create a skill file without reading this skill first.
---

# Skill Creator for zcode

This skill teaches you how to create skills that work correctly with zcode's skills system.

## Where skills live

Skills are discovered from TWO locations (both checked automatically):

| Location | Scope | Used for |
|----------|-------|----------|
| `.zcode/skills/<name>/SKILL.md` | Project only | Skills specific to this project |
| `~/.config/zcode/skills/<name>/SKILL.md` | All projects | Globally useful skills (like this one) |

## Skill file format

Every skill MUST have YAML frontmatter:

```markdown
---
name: my-skill
description: What this skill does and when to trigger it
disable-model-invocation: false  # optional, defaults to false
---

# Skill Title

Instructions go here...
```

Required fields:
- **`name`**: Unique identifier (kebab-case recommended, e.g. `fix-grammar`)
- **`description`**: When to trigger + what it does. This is the PRIMARY trigger — include specific contexts and keywords. Be a little "pushy" to avoid under-triggering. Example: *"Fix grammar and spelling in the current document. Use when the user mentions proofreading, grammar check, spelling, or wants to polish their writing."*

Optional fields:
- **`disable-model-invocation`**: Set to `true` to hide the skill from the AI (user can still enable/disable from Settings UI)

## Skill creation workflow

### 1. Capture intent
Ask the user:
- What should this skill do?
- When should it trigger? (what user phrases/contexts)
- What's the expected output?

### 2. Write the skill
- Create the directory: `.zcode/skills/<name>/` (project) or `~/.config/zcode/skills/<name>/` (global)
- Write `SKILL.md` with frontmatter + instructions
- Keep instructions clear and specific. Use imperative mood. Include examples.
- Explain *why* rather than just commanding — models respond better to understanding than rigid ALWAYS/NEVER rules.

### 3. Test
- Write the skill file using the `write` tool
- Then send a test message to verify the model picks up the skill.
- Ask the user if the behavior matches expectations.

### 4. Iterate
- Refine based on user feedback
- Delete and recreate if needed

## Tips

- **Descriptions matter most.** The description is what determines whether the skill gets triggered. Describe both what the skill does AND when to use it.
- **Keep skills focused.** One skill = one clear purpose. Don't cram unrelated instructions into one skill.
- **Project vs global:** Use project-level (`.zcode/skills/`) for project-specific workflows (coding conventions, domain knowledge). Use global (`~/.config/zcode/skills/`) for reusable skills that apply across projects.
- **Test with the actual agent.** After creating a skill, ask the user to send a message that should trigger it, and verify the agent follows the instructions.
