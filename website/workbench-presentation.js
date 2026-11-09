// Presentation guard for the Workbench. The page asks this module before it publishes any result, manifest,
// figure or control state. It is the interface-level check that complements the run id validated by the
// execution controller: a result also becomes stale when the user has started a newer run or changed project,
// even if the controller would have accepted the reply.
//
// Tickets: `begin()` starts a presentation for the current project and returns a ticket. A ticket is current
// while no newer ticket was issued and the project has not changed since it was issued.

export function createPresentationGuard() {
  let latest = 0;
  let projectGeneration = 0;

  return {
    // Starts a new presentation (a run, a manifest refresh). Earlier tickets become stale.
    begin() {
      latest += 1;
      return { ticket: latest, project: projectGeneration };
    },
    // True only for the most recent ticket and only while the project it was issued for is still open.
    isCurrent(ticket) {
      return ticket.ticket === latest && ticket.project === projectGeneration;
    },
    // A project was loaded, imported, reset or replaced: every ticket issued before is stale.
    changeProject() {
      projectGeneration += 1;
      latest += 1;
    },
    // Ends the presentation of the given ticket without starting another. Only the current one may end it.
    end(ticket) {
      if (this.isCurrent(ticket)) latest += 1;
    },
    get generation() {
      return projectGeneration;
    },
  };
}
