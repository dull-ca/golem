# Golem — terminology (Discworld)

## The Scroll

- **Scroll** — the instructions a user gives a golem: a self-contained
  description of a set of Hosts and the Workloads, Services, and Ingress on
  them — a small system the golem makes and keeps running. Each Scroll is
  sealed off from every other: things inside it share a private internal
  network (which can span several Hosts), and nothing outside gets in except
  through an Ingress. The seal holds even when two Scrolls share a Host.
  Inscribing the same name again replaces it.

## What a Scroll declares

A Scroll names Hosts; everything else lives on a Host.

- **Host** — a machine things run on, and the container for everything that
  runs there. A Host holds the Workloads, Services, and Ingress placed on it; a
  thing runs on the Host that names it.
- **Workload** — a container that runs but isn't attached to any network.
- **Service** — a container that runs and is on the Scroll's internal network
  (reachable from elsewhere in the same Scroll, not from outside).
- **Ingress** — how something is allowed into the Scroll's network, whether
  from the outside world or from elsewhere inside it.

## The work

- **inscribe / efface** — what a user does to a Scroll. Inscribe: ask for it
  (add it, or replace one with the same name). Efface: ask for it to be gone.
- **make / unmake** — what a golem does in response: building or removing the
  Scroll's contents, done by its hands.
- **Deed** — one recorded step in a make or unmake (for example, starting a
  Service or removing an Ingress).
- **Reckoning** — the resolved picture of what a golem should be running on its
  Host(s): the Workloads, Services, and Ingress currently called for, and which
  Scrolls call for each.
- **Stratum** — one layer added to the golem's record, one per change (Waking /
  Inscribe / Efface). Each layer holds the Reckoning and the Deeds at that
  point, and is never removed.

## A golem's anatomy

- **golem** — one running clay being, on one Host. It works on its own: given
  the agreed Scrolls, it builds its own Host's share without coordinating with
  anyone else. It is made of chem and hands.
- **chem** — the words in its head; they both power it and tell it what to do.
  They read the Scrolls, work out what the Host should run, drive the hands,
  and add the strata. The golem takes its Scrolls from the Scriptorium.
- **hands** — the clay that does the building and removing. The kind of clay a
  golem is made from decides how it works (trixie-clay, compose-clay).
- **the clays** — the kinds of clay a golem can be made from; a way to talk
  about the whole set.

## Among golems

- **Congregation** — the group of golems that share one set of Scrolls.
- **Scribe** — the golem that, for a while, writes the master copy the others
  follow. If it goes silent, the Congregation picks a new one.
- **Moot** — how the Congregation agrees: the Scribe's changes are passed
  around and copied, and a change takes effect once it has the Assent.
- **Assent** — the number of golems that must accept a change before it takes
  effect (and how a new Scribe is chosen).
- **Scriptorium** — a golem's own store of the Scrolls it keeps.
