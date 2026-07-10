+++
title = "MCP Is Not a New Paradigm: What It Actually Standardizes, and How to Design For It"
date = 2026-07-10T13:30:00+08:00
slug = "mcp-is-not-a-new-paradigm-what-it-actually-standardizes-and-how-to-design-for-it"

[taxonomies]
tags = ["mcp", "api-design", "llm"]
+++

## TL;DR

The Model Context Protocol (MCP) is best understood not as a new kind of API but as a **standardized profile of existing API patterns** — JSON-RPC 2.0, plus mandatory runtime introspection, plus Backend-for-Frontend–style aggregation — specialized for one unusual consumer: a stochastic, forgetful model that self-discovers your capabilities and re-reads your interface on every turn.

Almost all the mechanics transfer from ordinary API design. What is genuinely MCP-specific is narrow: enforced uniform discovery (so a generic client works against a server it has never seen), a few LLM-native semantics (sampling, elicitation, the tools/resources/prompts trichotomy), and the fact that your tool descriptions stop being documentation and become runtime input that steers model behavior.

---

## 1. What MCP is, mechanically

Strip the branding and MCP is:

- **JSON-RPC 2.0** as the wire format.
- A **fixed, standardized method vocabulary**: `initialize`, `tools/list`, `tools/call`, `resources/read`, `prompts/list`, and so on.
- A **mandatory discovery handshake**: a client connects knowing nothing, calls `tools/list`, and receives the full menu with machine-readable JSON schemas.

Your `add(a, b)` function is not reachable at some URL you chose. It is reachable via `tools/call` with `{"name": "add", ...}`. The consumer is an LLM/agent that does not know your API in advance: it connects, enumerates capabilities, reads the schemas, and decides at runtime what to invoke.

MCP exposes three primitives:

- **Tools** — actions the model can invoke.
- **Resources** — read-only context the model or client can pull.
- **Prompts** — reusable templates/instructions.

Transport can be stdio, streamable HTTP, or SSE. HTTP is just one option; MCP is not "an HTTP API."

---

## 2. MCP vs REST: who is the caller?

Both speak over a transport. They answer different questions.

| | REST / OpenAPI | MCP |
|---|---|---|
| Standardizes | Exposing **resources** to programs | Exposing **capabilities** to models |
| Consumer | A developer who reads docs, holds state, orchestrates deliberately | A model that sees a flat menu, picks under token pressure, has no memory of your workflow |
| Contract delivery | **Out-of-band** — you must already hold the endpoints/shapes before you can call | **In-band** — the contract travels with the connection; discovered through the communication itself |
| Discovery | Optional, heterogeneous (some ship OpenAPI, some don't) | **Mandatory and uniform** — the reason a generic client can connect to any server |
| Granularity unit | One resource operation (`POST /carts/{id}/items`) | One unit of user intent (`place_order`) |

The relationship is clearest in one fact: **FastMCP can generate an MCP server from a FastAPI app or an OpenAPI spec.** MCP sits a layer up. FastAPI is how you expose an API to programs you control; MCP is how you expose capabilities to an agent you don't, in a form it can enumerate and reason about.

**Rule of thumb:** if a service is called by code you write, plain REST/gRPC is simpler and faster. Reach for MCP when the caller is a model that needs to *discover and choose* tools. For deterministic plumbing between machines you control, MCP adds protocol overhead you don't need.

---

## 3. The in-band vs out-of-band distinction

- **MCP is self-describing.** A caller with zero prior knowledge can obtain the full capability set purely through the communication. Discovery is part of the required method set.
- **Plain REST is prior-knowledge.** Nothing in the wire format obligates the server to describe itself, so the caller must already possess the contract. OpenAPI exists, but it's a separate document a human fetches and wires up ahead of time — bolted on, aimed at humans and build-time codegen, not intrinsic to the call vocabulary.

**Important correction to a common overstatement:** in-band runtime discovery is *not* unique to MCP. GraphQL introspection queries the schema at runtime through the same endpoint. gRPC server reflection enumerates services and methods without prior stubs. Both are self-describing and discovered through the communication itself. The property is a *general* API capability that MCP happens to **require**. gRPC *can* do reflection; MCP *mandates* it. That mandate — not the capability itself — is what lets a generic client work everywhere.

---

## 4. The dominant pattern (wrap an existing API) and its failure mode

Most MCP servers in the wild wrap an existing API. That's not wrong. The failure mode is doing it **too literally**: a 1:1 mapping where every REST endpoint becomes one tool.

The temptation is natural because REST endpoints and MCP tools are both "callable operations," and OpenAPI-to-MCP generators produce exactly this. It's a great *starting point* and a fine way to see what a model actually reaches for. But an API contract is designed for a programmer who reads docs, holds state, and sequences calls deliberately. An MCP tool is consumed by a model that sees a flat menu, picks under token pressure, and has no memory of your intended workflow. Different audiences.

Where the thin wrapper goes wrong:

- **Granularity mismatch.** A workflow that a human chains correctly (create cart → add items → apply coupon → checkout) becomes a call-ordering puzzle the model fumbles. Often the right tool is one that hides the choreography.
- **Schema noise.** Auto-generated tools inherit every optional field, pagination param, and internal ID. That's schema the model must read and reason over every turn — token cost and a source of wrong calls.
- **Return shape.** APIs return complete objects; a model usually needs a compact projection, not the raw payload.
- **Tool-count blowup.** 60 endpoints → 60 tools degrades selection accuracy. Curation and tag-based filtering matter.

**Better framing:** the API is the *transport layer underneath* your MCP server, not the design of it. Wrap the API, but **redesign the surface** — task-oriented tools, trimmed schemas, projected outputs — treating the model as the user you're doing UX for.

**Gut check:** if a competent teammate would need your API docs open to use the tool correctly, the model will struggle too.

---

## 5. Worked example: order management

### The underlying REST API

```
POST /carts                      → {cart_id}
POST /carts/{id}/items           → add one line item
GET  /catalog/products?q=...     → search, paginated
POST /carts/{id}/apply-coupon    → {discount}
GET  /carts/{id}                 → full cart w/ totals
POST /carts/{id}/checkout        → requires payment_method_id
```

### Thin 1:1 wrapper (the tempting mistake)

```python
@mcp.tool
def create_cart() -> dict: ...

@mcp.tool
def add_item(cart_id: str, product_id: str, qty: int,
             gift_wrap: bool = False, warehouse_hint: str | None = None,
             price_override_cents: int | None = None) -> dict: ...

@mcp.tool
def search_products(q: str, page: int = 1, page_size: int = 20,
                    sort: str = "relevance", locale: str = "en-US") -> dict: ...

@mcp.tool
def apply_coupon(cart_id: str, code: str) -> dict: ...

@mcp.tool
def get_cart(cart_id: str) -> dict: ...

@mcp.tool
def checkout(cart_id: str, payment_method_id: str) -> dict: ...
```

To buy two items with a coupon, the model must: create a cart, thread the returned `cart_id` into search, parse a paginated result to pick a `product_id`, add each item, apply the coupon, then check out with a `payment_method_id` it has to source from somewhere. Six-plus turns, strict ordering, hand-carried state. Every schema carries noise (`warehouse_hint`, `price_override_cents`, `locale`, pagination) the model must read past. And `get_cart` returns the whole object when the model wanted one number.

### Redesigned surface (task-oriented)

```python
@mcp.tool
def find_products(query: str, max_results: int = 5) -> list[dict]:
    """Search the catalog. Returns [{product_id, name, price}]."""
    raw = api.get("/catalog/products", params={"q": query, "page_size": max_results})
    # projection: drop everything the model doesn't need to decide
    return [{"product_id": p["id"], "name": p["name"],
             "price": p["price_cents"] / 100} for p in raw["items"]]

@mcp.tool
def place_order(items: list[OrderItem], coupon: str | None = None,
                payment_method_id: str = Depends(get_default_payment)) -> dict:
    """Create a cart, add all items, apply an optional coupon, and check out
    in one call. items = [{product_id, quantity}]. Returns {order_id, total, status}."""
    cart = api.post("/carts")["cart_id"]
    for it in items:
        api.post(f"/carts/{cart}/items",
                 json={"product_id": it.product_id, "qty": it.quantity})
    if coupon:
        api.post(f"/carts/{cart}/apply-coupon", json={"code": coupon})
    result = api.post(f"/carts/{cart}/checkout",
                      json={"payment_method_id": payment_method_id})
    return {"order_id": result["id"], "total": result["total_cents"] / 100,
            "status": result["status"]}
```

Two tools instead of six. The moves that mattered:

- **Collapsed the workflow** into one tool so call-ordering can't be gotten wrong.
- **`Depends(get_default_payment)`** injects the payment method server-side, so it never appears in the schema and can't be hallucinated or leaked.
- **Projected outputs** — three fields, not the raw paginated payload; a 3-key summary, not the full order object.
- **Dropped internal knobs** (`warehouse_hint`, `price_override_cents`) from the model-facing surface. They still exist in the API; the model just doesn't see them.

In FastMCP 3.0 you express this without hand-writing every wrapper: source the raw tools from the OpenAPI spec via a **Provider**, then apply **Transforms** — `place_order` is a custom tool, `find_products` is the raw endpoint with an output transform plus hidden params, and internal endpoints get filtered out by tag. The API stays the transport; the Transform chain is where the model-facing UX lives.

---

## 6. Does this violate Single Responsibility?

No — it **relocates** it. SRP is about one reason to change, not one HTTP call. `place_order` has a single responsibility: "fulfill the intent of ordering." Making four API calls internally is an implementation detail, exactly like a service-layer `OrderService.placeOrder()` that touches carts, coupons, and payments. Nobody calls that an SRP violation.

The unit SRP applies to differs by layer:

- **REST endpoint:** one responsibility = one resource operation.
- **MCP tool:** one responsibility = one unit of user intent.
- **The thin 1:1 wrapper** actually has *worse* cohesion — it spreads one responsibility (placing an order) across six tools plus the model's reasoning, forcing the model to become the orchestrator. That's the real SRP smell.

SRP is defined relative to the consumer. The consumer changed, so the grain got coarser. Same principle.

---

## 7. Is MCP "a standard for API design"?

It's a **wire protocol standard**: it standardizes the transport, the method vocabulary, and the discovery handshake. It does **not** standardize how you design the tool surface. No spec clause says "collapse workflows" or "project outputs." All the design advice above is convention and emerging best practice, not protocol. **MCP standardizes the envelope, not the content.**

Consequence: a 1:1 REST-wrapping server is fully protocol-compliant *and* often badly designed — the same way a REST API with 40 endpoints named `/doStuff1`, `/doStuff2` is valid HTTP and bad design. **Compliance and good design are orthogonal.**

---

## 8. So are MCP's design patterns just general API design?

Largely, yes — this is the key realization.

The design advice (collapse workflows, project outputs, coarse task-level granularity) is the **Backend-for-Frontend (BFF)** pattern. An MCP server is essentially a **"Backend for Model"**: an aggregation/adaptation layer shaped for one specific consumer, exactly like a BFF aggregates chatty microservice calls into one endpoint tuned for a mobile client. `place_order` isn't an MCP idea; it's good API design that a forgiving human developer will tolerate you skipping and a model won't.

What is *genuinely* MCP-specific is narrow:

1. **Standardization / universality.** REST discovery is optional and heterogeneous. MCP mandates one uniform discovery handshake — the whole reason a generic client can connect to any server it's never seen. A property of the *standard*, not the pattern.
2. **A few LLM-native semantics.** **Sampling** (the server asks the client's LLM to generate mid-call) and **elicitation** (the server asks the client to prompt its user) have no clean equivalent in a normal API. Even these are callback patterns underneath, but the tools/resources/prompts trichotomy is a deliberate semantic layer aimed at a model.
3. **Descriptions become runtime input, not docs.** The subtle one. In a human API, a field description is documentation — read once, then ignored at runtime. In MCP, the tool description and schema are injected into the model's context and *steer selection on every call*. Your naming and docstrings stop being annotation and become part of the program's behavior. No conventional API has this property, because human consumers don't recompile their intent from your docstring each turn.

---

## 9. The one-liner to remember

> **MCP is a standardized profile of existing API patterns (JSON-RPC + mandatory introspection + BFF-style aggregation), specialized for a stochastic, forgetful, self-discovering consumer.**

The mechanics transfer almost entirely. What doesn't reduce is the *enforced uniformity* (so a generic client works everywhere) and the fact that your interface copy is now runtime input rather than reference material.

Push it one step further and most of "MCP design" is really just **"API design where the consumer can't read your docs, won't hold state for you, and re-reads the menu every turn."** Constraints, not a new discipline.

---

## Design checklist for an MCP server

- [ ] Are tools shaped around **units of intent**, not units of REST?
- [ ] Have you **collapsed multi-step choreography** so the model can't mis-order calls?
- [ ] Are **schemas trimmed** to only what the model should decide on? Internal knobs hidden?
- [ ] Are **outputs projected** to the relevant fields, not raw payloads?
- [ ] Is the **tool count** small enough to preserve selection accuracy? (Curate / tag-filter.)
- [ ] Are secrets/credentials/IDs **injected server-side** (e.g. dependency injection) rather than exposed in schemas?
- [ ] Do **tool names and descriptions** read as clear instructions to a model, given they're runtime input?
- [ ] Gut check: could a teammate use each tool **without your API docs open**?
