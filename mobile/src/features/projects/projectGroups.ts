import type { ProjectSummary } from '../../types';

export type ProjectGroup = {
  key: string;
  label: string;
  grouped: boolean;
  projects: ProjectSummary[];
};

export function groupProjects(
  visibleProjects: ProjectSummary[],
  allProjects: ProjectSummary[],
): ProjectGroup[] {
  const repositoryLabels = new Map<string, string>();
  for (const project of allProjects) {
    const repository = project.worktree?.repository;
    if (repository && !project.worktree?.discovered) {
      repositoryLabels.set(repository, project.name);
    }
  }

  const groups = new Map<string, ProjectGroup>();
  for (const project of visibleProjects) {
    const repository = project.worktree?.repository;
    const key = repository ?? `project:${project.name}`;
    const group = groups.get(key) ?? {
      key,
      label: repositoryLabels.get(key) ?? project.name,
      grouped: false,
      projects: [],
    };
    group.projects.push(project);
    groups.set(key, group);
  }

  return [...groups.values()].map((group) => ({
    ...group,
    grouped: group.projects.length > 1,
  }));
}
