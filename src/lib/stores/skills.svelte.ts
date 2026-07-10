import { invoke } from "@tauri-apps/api/core";

export interface SkillWithState {
  name: string;
  description: string;
  file_path: string;
  base_dir: string;
  source: string; // "builtin" | "user" | "project"
  disable_model_invocation: boolean;
  active: boolean;
}

let skills = $state<SkillWithState[]>([]);

export const skillsStore = {
  get all() {
    return skills;
  },

  get enabled() {
    return skills.filter((s) => s.active);
  },

  async reload(cwd: string) {
    try {
      skills = await invoke<SkillWithState[]>("list_skills", { cwd });
    } catch (err) {
      console.error("[skills] reload failed:", err);
      skills = [];
    }
  },

  async toggle(name: string, active: boolean, cwd: string) {
    try {
      await invoke("set_skill_active", { name, active });
      await this.reload(cwd);
    } catch (err) {
      console.error("[skills] toggle failed:", err);
    }
  },
};
