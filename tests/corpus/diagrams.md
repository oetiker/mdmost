# Feature exerciser — Mermaid

All seven families, each exercising the syntax the README claims to support,
each small enough that its box art can be read as a diff. Wide and degenerate
cases are at the end.

## flowchart — shapes and edge kinds

```mermaid
flowchart TD
  A[rect] --> B(round)
  B --> C([stadium])
  C --> D{rhombus}
  D -->|yes| E((circle))
  D -- no --> F[[subroutine]]
  F -.-> G[(cylinder)]
  G ==> H[end]
  A --- H
```

## flowchart — left to right, with a subgraph

```mermaid
flowchart LR
  subgraph Outer
    A[one] --> B[two]
    subgraph Inner
      C[three]
    end
    B --> C
  end
  C --> D[four]
```

## sequenceDiagram — arrows, notes and blocks

```mermaid
sequenceDiagram
  participant A as Ana
  actor B as Bo
  A->>B: solid open
  B-->>A: dashed open
  A-x B: solid cross
  A->>A: a self message
  activate B
  Note over A,B: a note over both
  Note right of B: a note on one side
  alt happy path
    B->>A: ok
  else sad path
    B->>A: not ok
  end
  loop three times
    A->>B: again
  end
  deactivate B
```

## classDiagram — compartments, visibility, stereotypes, relations

```mermaid
classDiagram
  class Shape {
    <<interface>>
    +String name
    #int sides
    -bool secret
    +area()* f64
    +create()$ Shape
  }
  class Circle~T~ {
    +f64 radius
  }
  Shape <|-- Circle
  Circle *-- Point
  Circle o-- Style
  Circle --> "1" Canvas
  Circle ..> Helper
  Circle ..|> Drawable
```

## erDiagram — attributes, keys, cardinalities

```mermaid
erDiagram
  AUTHOR ||--o{ BOOK : writes
  BOOK }o--|| PUBLISHER : "printed by"
  AUTHOR {
    int id PK
    string name "the display name"
    string email UK
  }
  BOOK {
    int isbn PK
    int author_id FK
  }
```

## stateDiagram-v2 — composites, choice, fork, notes

```mermaid
stateDiagram-v2
  [*] --> Idle
  Idle --> Busy : start
  state Busy {
    [*] --> Working
    Working --> Waiting : block
    Waiting --> Working : wake
  }
  state pick <<choice>>
  Busy --> pick
  pick --> Idle : more
  pick --> [*] : done
  note right of Idle : nothing to do
```

## pie

```mermaid
pie showData title Effort by stage
  "Design" : 3
  "Build" : 5
  "Review" : 2
  "Ship" : 1
```

## gantt — sections, tags, dependencies

```mermaid
gantt
  title Release plan
  dateFormat YYYY-MM-DD
  axisFormat %m-%d
  section Design
  sketch      :done,   a1, 2026-01-01, 3d
  review      :active, a2, after a1, 2d
  section Build
  implement   :crit,   a3, after a2, 5d
  ship        :milestone, after a3, 0d
```

## Directives and comments are parsed and ignored

```mermaid
%%{init: {"theme": "dark"}}%%
flowchart TD
  %% this comment must not appear
  A[after a directive] --> B[and a comment]
```

## Wider than eighty columns

```mermaid
flowchart LR
  Ingest[Ingest the source] --> Parse[Parse to an AST] --> Layout[Negotiate widths] --> Draw[Draw the canvas] --> Paint[Paint the viewport]
```

## Degenerate — a single node

```mermaid
flowchart TD
  Only[Just one node]
```

## Degenerate — an empty diagram

```mermaid
flowchart TD
```

## Not a supported diagram at all

```mermaid
journey
  title My working day
  section Go to work
    Make tea: 5: Me
```

## Not valid mermaid at all

```mermaid
this is not valid mermaid at all
```
