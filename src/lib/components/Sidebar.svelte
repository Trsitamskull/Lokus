<script>
  import { tick, untrack } from "svelte";
  import {
    Scale,
    Plus,
    X,
    FileSpreadsheet,
    Folder,
    FolderOpen,
    FolderPlus,
    FolderInput,
    ChevronRight,
    ChevronDown,
    ChevronsDownUp,
    ChevronsUpDown,
    Pencil,
    Palette,
    Search,
    UserCheck,
    History,
    Download,
    Trash2,
    Keyboard,
    Sun,
    Moon,
  } from "@lucide/svelte";

  let {
    archivosDisponibles = [],
    carpetasDisponibles = [],
    ordenCarpetas = [],
    coloresCarpetas = {},
    archivoActual = null,
    totales,
    aiStatus,
    theme = "dark",
    mobileOpen = false,
    onCloseMobile,
    onCrearArchivo,
    onCrearCarpeta,
    onRenombrarCarpeta,
    onCambiarColorCarpeta,
    onEliminarCarpeta,
    onMoverArchivo,
    onMoverPorArrastre,
    onReordenarCarpetas,
    onSeleccionarArchivo,
    onEditarArchivo = null,
    onAbrirDefensorArchivo,
    onAbrirRespaldos,
    onDescargarArchivo,
    onEliminarArchivo,
    onAbrirConfigIa,
    onAbrirAtajos,
    onToggleTema,
  } = $props();

  let busquedaPlanillas = $state("");
  let carpetasExpandidas = $state({});
  let filesContainerEl = $state(null);

  // Menú contextual personalizado (carpetas y planillas)
  let contextMenu = $state(null); // { tipo: "carpeta" | "archivo", ruta, x, y, viaKeyboard, triggerEl }
  let contextAnchorEl = $state(null);
  let contextMenuEl = $state(null);
  let contextMenuFocusgroupEl = $state(null);
  let restaurarFocoAlCerrar = true;

  // Estado de arrastre mediante Pointer Events (Windows WebView2 y Linux WebKitGTK)
  let posibleArrastre = $state(null); // { tipo, ruta, nombre, carpetaOrigen, startX, startY, pointerId, pointerType }
  let arrastreActivo = $state(null);  // { tipo, ruta, nombre, carpetaOrigen, x, y, pointerId }
  let hoverDestino = $state(null);    // "" (raíz) | "2026/Enero" | null
  let hoverPosicion = $state(null);   // "antes" | "dentro" | "despues" | null
  let bloquearClickTemporal = false;
  let autoExpandTimer = null;
  let autoExpandTarget = null;

  function normalizar(texto) {
    return String(texto || "")
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .toLowerCase();
  }

  function padreDeRuta(ruta) {
    const idx = ruta.lastIndexOf("/");
    return idx >= 0 ? ruta.slice(0, idx) : "";
  }

  const COLORES_VALIDOS = new Set([
    "azul",
    "verde",
    "ambar",
    "coral",
    "rosa",
    "violeta",
    "turquesa",
  ]);

  function estiloColorCarpeta(ruta) {
    const color = coloresCarpetas?.[ruta];
    return color && COLORES_VALIDOS.has(color)
      ? `--folder-color: var(--folder-${color})`
      : "";
  }

  let ultimoArchivoAutoExpandido = "";

  // Expandir automáticamente las carpetas padre solo cuando cambia la planilla activa
  // (usando untrack sobre carpetasExpandidas para permitir plegarla manualmente después)
  $effect(() => {
    const rutaActiva = archivoActual || "";
    if (rutaActiva === ultimoArchivoAutoExpandido) return;
    ultimoArchivoAutoExpandido = rutaActiva;
    if (!rutaActiva.includes("/")) return;

    const partes = rutaActiva.split("/").slice(0, -1);
    untrack(() => {
      let acum = "";
      let cambios = false;
      const siguiente = { ...carpetasExpandidas };
      for (const p of partes) {
        acum = acum ? `${acum}/${p}` : p;
        if (!siguiente[acum]) {
          siguiente[acum] = true;
          cambios = true;
        }
      }
      if (cambios) {
        carpetasExpandidas = siguiente;
      }
    });
  });

  function contieneArchivoActivo(rutaCarpeta) {
    return Boolean(archivoActual && archivoActual.startsWith(`${rutaCarpeta}/`));
  }

  let hayAlgunaCarpetaExpandida = $derived(
    carpetasDisponibles.some((c) => Boolean(carpetasExpandidas[c]))
  );

  function alternarTodasCarpetas() {
    if (hayAlgunaCarpetaExpandida) {
      carpetasExpandidas = {};
    } else {
      const todas = {};
      for (const c of carpetasDisponibles) {
        todas[c] = true;
      }
      carpetasExpandidas = todas;
    }
  }

  function construirYFiltrarArbol(carpetas, archivos, consulta, orden) {
    const tokens = normalizar(consulta.trim()).split(/\s+/).filter(Boolean);
    const hayFiltro = tokens.length > 0;
    const ordenMap = new Map(orden.map((ruta, i) => [ruta, i]));

    function crearNodo(nombre, ruta) {
      return {
        nombre,
        ruta,
        carpetasMap: new Map(),
        carpetas: [],
        archivos: [],
        totalArchivos: 0,
      };
    }

    const raiz = crearNodo("", "");

    function asegurarCarpeta(rutaCarpeta) {
      if (!rutaCarpeta) return raiz;
      const partes = rutaCarpeta.split("/").filter(Boolean);
      let actual = raiz;
      let acum = "";
      for (const parte of partes) {
        acum = acum ? `${acum}/${parte}` : parte;
        if (!actual.carpetasMap.has(parte)) {
          actual.carpetasMap.set(parte, crearNodo(parte, acum));
        }
        actual = actual.carpetasMap.get(parte);
      }
      return actual;
    }

    for (const c of carpetas) {
      asegurarCarpeta(c);
    }

    for (const arch of archivos) {
      const idx = arch.lastIndexOf("/");
      const dir = idx >= 0 ? arch.slice(0, idx) : "";
      const nombre = idx >= 0 ? arch.slice(idx + 1) : arch;
      asegurarCarpeta(dir).archivos.push({ ruta: arch, nombre });
    }

    function ordenarYContar(nodo) {
      const hijos = [...nodo.carpetasMap.values()].sort((a, b) => {
        const ia = ordenMap.has(a.ruta) ? ordenMap.get(a.ruta) : Number.MAX_SAFE_INTEGER;
        const ib = ordenMap.has(b.ruta) ? ordenMap.get(b.ruta) : Number.MAX_SAFE_INTEGER;
        if (ia !== ib) return ia - ib;
        return a.nombre.toLowerCase().localeCompare(b.nombre.toLowerCase());
      });
      nodo.archivos.sort((a, b) =>
        a.nombre.toLowerCase().localeCompare(b.nombre.toLowerCase())
      );

      let total = nodo.archivos.length;
      const hijosFiltrados = [];

      for (const h of hijos) {
        ordenarYContar(h);
        if (!hayFiltro) {
          hijosFiltrados.push(h);
          total += h.totalArchivos;
        } else {
          const carpetaCoincide = tokens.every((t) =>
            normalizar(h.ruta).includes(t)
          );
          if (carpetaCoincide || h.carpetas.length > 0 || h.archivos.length > 0) {
            hijosFiltrados.push(h);
            total += h.totalArchivos;
          }
        }
      }

      if (hayFiltro) {
        const padreCoincide =
          nodo.ruta !== "" &&
          tokens.every((t) => normalizar(nodo.ruta).includes(t));
        if (!padreCoincide) {
          nodo.archivos = nodo.archivos.filter((a) =>
            tokens.every((t) => normalizar(a.ruta).includes(t))
          );
        }
        total = nodo.archivos.length;
        for (const h of hijosFiltrados) {
          total += h.totalArchivos;
        }
      }

      nodo.carpetas = hijosFiltrados;
      nodo.totalArchivos = total;
    }

    ordenarYContar(raiz);
    return { raiz, hayFiltro };
  }

  let arbolEstado = $derived(
    construirYFiltrarArbol(
      carpetasDisponibles,
      archivosDisponibles,
      busquedaPlanillas,
      ordenCarpetas
    )
  );

  function toggleCarpeta(ruta) {
    carpetasExpandidas = {
      ...carpetasExpandidas,
      [ruta]: !carpetasExpandidas[ruta],
    };
  }

  function esDestinoValido(arrastre, destino, posicion) {
    if (!arrastre || destino === null || destino === undefined || !posicion) {
      return false;
    }

    if (arrastre.tipo === "archivo") {
      return posicion === "dentro" && destino !== arrastre.carpetaOrigen;
    }

    // Arrastre de carpeta
    if (posicion === "dentro") {
      if (destino === arrastre.carpetaOrigen) return false;
      if (destino === arrastre.ruta) return false;
      if (destino.startsWith(`${arrastre.ruta}/`)) return false;
      return true;
    }

    // Reordenamiento "antes" / "despues" relativo a otra carpeta `destino`
    if (!destino) return false;
    if (destino === arrastre.ruta) return false;
    if (destino.startsWith(`${arrastre.ruta}/`)) return false;
    return true;
  }

  function iniciarPosibleArrastre(e, item) {
    if (e.button !== 0) return;
    if (e.pointerType === "mouse") {
      window.getSelection?.()?.removeAllRanges?.();
    }
    posibleArrastre = {
      tipo: item.tipo,
      ruta: item.ruta,
      nombre: item.nombre,
      carpetaOrigen: padreDeRuta(item.ruta),
      startX: e.clientX,
      startY: e.clientY,
      pointerId: e.pointerId,
      pointerType: e.pointerType,
    };
  }

  function limpiarAutoExpand() {
    if (autoExpandTimer) {
      clearTimeout(autoExpandTimer);
      autoExpandTimer = null;
    }
    autoExpandTarget = null;
  }

  function finalizarEstadoArrastre() {
    const pid = arrastreActivo?.pointerId ?? posibleArrastre?.pointerId;
    if (pid != null && filesContainerEl?.hasPointerCapture?.(pid)) {
      try {
        filesContainerEl.releasePointerCapture(pid);
      } catch (_) {}
    }
    limpiarAutoExpand();
    posibleArrastre = null;
    arrastreActivo = null;
    hoverDestino = null;
    hoverPosicion = null;
    document.body.classList.remove("is-dragging-tree");
  }

  function calcularDestinoDesdePunto(clientX, clientY, arrastre) {
    if (!arrastre || !Number.isFinite(clientX) || !Number.isFinite(clientY)) {
      return { destino: null, posicion: null };
    }
    const el = document.elementFromPoint(clientX, clientY);
    const zona = el?.closest?.("[data-drop-folder]");
    const destino = zona ? zona.getAttribute("data-drop-folder") : null;
    if (destino === null) {
      return { destino: null, posicion: null };
    }
    if (
      destino === "" ||
      zona.hasAttribute("data-drop-only-inside") ||
      arrastre.tipo !== "carpeta"
    ) {
      return { destino, posicion: "dentro" };
    }
    const rect = zona.getBoundingClientRect();
    const relY = (clientY - rect.top) / Math.max(1, rect.height);
    if (relY < 0.28) return { destino, posicion: "antes" };
    if (relY > 0.72) return { destino, posicion: "despues" };
    return { destino, posicion: "dentro" };
  }

  function calcularNuevoOrdenCarpetas(rutaMovida, destino, posicion) {
    const parentTarget = padreDeRuta(destino);
    // Construir árbol base sin mover aún o con la ruta ya reflejada en carpetasDisponibles
    const baseCarpetas = carpetasDisponibles.includes(rutaMovida)
      ? carpetasDisponibles
      : [...carpetasDisponibles, rutaMovida];
    const { raiz } = construirYFiltrarArbol(baseCarpetas, [], "", ordenCarpetas);

    const salida = [];
    function recorrer(nodo) {
      let listaHijos = nodo.carpetas.map((h) => h.ruta);
      if (nodo.ruta === parentTarget) {
        const sinMovida = listaHijos.filter((r) => r !== rutaMovida);
        const idx = sinMovida.indexOf(destino);
        if (idx >= 0) {
          const insertarEn = posicion === "antes" ? idx : idx + 1;
          sinMovida.splice(insertarEn, 0, rutaMovida);
          listaHijos = sinMovida;
        }
      }
      for (const rHijo of listaHijos) {
        salida.push(rHijo);
        const hijoNodo = nodo.carpetas.find((h) => h.ruta === rHijo);
        if (hijoNodo) recorrer(hijoNodo);
      }
    }
    recorrer(raiz);
    return salida;
  }

  function manejarPointerMoveGlobal(e) {
    if (!posibleArrastre && !arrastreActivo) return;
    const esperado = arrastreActivo?.pointerId ?? posibleArrastre?.pointerId;
    if (esperado != null && e.pointerId != null && e.pointerId !== esperado) return;
    if (arrastreActivo && e.isTrusted && e.pointerType === "mouse" && e.buttons === 0) {
      manejarPointerUpGlobal(e);
      return;
    }

    if (!arrastreActivo && posibleArrastre) {
      const dist = Math.hypot(
        e.clientX - posibleArrastre.startX,
        e.clientY - posibleArrastre.startY
      );
      if (dist < 5) return;
      if (e.pointerType === "mouse") {
        window.getSelection?.()?.removeAllRanges?.();
      }
      arrastreActivo = {
        tipo: posibleArrastre.tipo,
        ruta: posibleArrastre.ruta,
        nombre: posibleArrastre.nombre,
        carpetaOrigen: posibleArrastre.carpetaOrigen,
        x: e.clientX,
        y: e.clientY,
        pointerId: posibleArrastre.pointerId ?? e.pointerId,
      };
      if (e.pointerId != null) {
        try {
          filesContainerEl?.setPointerCapture?.(e.pointerId);
        } catch (_) {}
      }
      document.body.classList.add("is-dragging-tree");
    }

    if (!arrastreActivo) return;
    arrastreActivo = { ...arrastreActivo, x: e.clientX, y: e.clientY };

    if (filesContainerEl) {
      const rect = filesContainerEl.getBoundingClientRect();
      if (e.clientY >= rect.top && e.clientY <= rect.top + 32) {
        filesContainerEl.scrollTop -= 8;
      } else if (e.clientY <= rect.bottom && e.clientY >= rect.bottom - 32) {
        filesContainerEl.scrollTop += 8;
      }
    }

    const { destino, posicion } = calcularDestinoDesdePunto(
      e.clientX,
      e.clientY,
      arrastreActivo
    );

    hoverDestino = destino;
    hoverPosicion = posicion;

    if (destino && posicion === "dentro" && !carpetasExpandidas[destino]) {
      if (autoExpandTarget !== destino) {
        limpiarAutoExpand();
        autoExpandTarget = destino;
        autoExpandTimer = setTimeout(() => {
          carpetasExpandidas = { ...carpetasExpandidas, [destino]: true };
          autoExpandTarget = null;
        }, 500);
      }
    } else {
      limpiarAutoExpand();
    }
  }

  async function manejarPointerUpGlobal(e) {
    if (!posibleArrastre && !arrastreActivo) return;
    const esperado = arrastreActivo?.pointerId ?? posibleArrastre?.pointerId;
    if (esperado != null && e?.pointerId != null && e.pointerId !== esperado) return;

    if (arrastreActivo) {
      const arrastre = arrastreActivo;
      const { destino, posicion } =
        Number.isFinite(e?.clientX) && Number.isFinite(e?.clientY)
          ? calcularDestinoDesdePunto(e.clientX, e.clientY, arrastre)
          : { destino: hoverDestino, posicion: hoverPosicion };
      bloquearClickTemporal = true;
      setTimeout(() => {
        bloquearClickTemporal = false;
      }, 60);

      finalizarEstadoArrastre();

      if (esDestinoValido(arrastre, destino, posicion)) {
        if (posicion === "dentro") {
          if (destino) {
            carpetasExpandidas = { ...carpetasExpandidas, [destino]: true };
          }
          await onMoverPorArrastre?.(arrastre, destino);
        } else if (arrastre.tipo === "carpeta" && destino) {
          const parentTarget = padreDeRuta(destino);
          let rutaFinal = arrastre.ruta;
          if (arrastre.carpetaOrigen !== parentTarget) {
            const movida = await onMoverPorArrastre?.(arrastre, parentTarget);
            if (!movida) return;
            rutaFinal = movida;
          }
          const nuevoOrden = calcularNuevoOrdenCarpetas(
            rutaFinal,
            destino,
            posicion
          );
          await onReordenarCarpetas?.(nuevoOrden);
        }
      }
      return;
    }

    finalizarEstadoArrastre();
  }

  function manejarKeydownVentana(e) {
    if (e.key === "Escape" && arrastreActivo) {
      e.preventDefault();
      e.stopPropagation();
      finalizarEstadoArrastre();
      return;
    }
    if (e.key === "Escape" && contextMenu) {
      e.preventDefault();
      e.stopPropagation();
      cerrarMenuContextual({ restaurarFoco: true });
    }
  }

  function esSoporteFocusgroup() {
    return typeof HTMLElement !== "undefined" && "focusGroup" in HTMLElement.prototype;
  }

  function esSoportePopover() {
    return typeof HTMLElement !== "undefined" && "popover" in HTMLElement.prototype;
  }

  function sincronizarFallbackMenu() {
    if (!contextMenuFocusgroupEl || esSoporteFocusgroup()) return;
    contextMenuFocusgroupEl.setAttribute("role", "menu");
    contextMenuFocusgroupEl.querySelectorAll("button").forEach((btn) => {
      if (!btn.getAttribute("role")) btn.setAttribute("role", "menuitem");
    });
  }

  function posicionarMenuFallback(x, y) {
    if (!contextMenuEl) return;
    const soportaAnchor =
      typeof CSS !== "undefined" && CSS.supports?.("position-area: block-end");
    if (soportaAnchor && esSoportePopover()) {
      contextMenuEl.style.left = "";
      contextMenuEl.style.top = "";
      return;
    }
    const rect = contextMenuEl.getBoundingClientRect();
    const ancho = rect.width || 210;
    const alto = rect.height || 210;
    const maxX = Math.max(8, window.innerWidth - ancho - 8);
    const left = Math.min(Math.max(8, x), maxX);
    let top = y;
    if (top + alto > window.innerHeight - 8) {
      top = Math.max(8, y - alto);
    }
    contextMenuEl.style.left = `${left}px`;
    contextMenuEl.style.top = `${top}px`;
  }

  async function abrirMenuContextual(e, tipo, ruta, triggerEl, viaKeyboard = false) {
    e.preventDefault();
    e.stopPropagation();
    if (arrastreActivo) return;

    let x = e.clientX ?? 0;
    let y = e.clientY ?? 0;
    if (viaKeyboard && triggerEl) {
      const rect = triggerEl.getBoundingClientRect();
      x = Math.round(rect.left + 28);
      y = Math.round(rect.bottom - 2);
    }

    restaurarFocoAlCerrar = true;
    contextMenu = { tipo, ruta, x, y, viaKeyboard, triggerEl };
    await tick();

    if (contextAnchorEl) {
      contextAnchorEl.style.left = `${x}px`;
      contextAnchorEl.style.top = `${y}px`;
    }
    sincronizarFallbackMenu();

    if (contextMenuEl && esSoportePopover()) {
      if (contextMenuEl.matches(":popover-open")) {
        restaurarFocoAlCerrar = false;
        try {
          contextMenuEl.hidePopover();
        } catch (_) {}
        restaurarFocoAlCerrar = true;
      }
      try {
        contextMenuEl.showPopover({ source: contextAnchorEl || undefined });
      } catch (_) {
        try {
          contextMenuEl.showPopover();
        } catch (_) {}
      }
    }

    posicionarMenuFallback(x, y);

    if (viaKeyboard) {
      const primero = contextMenuFocusgroupEl?.querySelector("button:not(:disabled)");
      primero?.focus();
    } else {
      contextMenuFocusgroupEl?.focus({ focusVisible: false });
    }
  }

  function cerrarMenuContextual({ restaurarFoco = false } = {}) {
    const info = contextMenu;
    restaurarFocoAlCerrar = restaurarFoco;
    if (contextMenuEl && esSoportePopover() && contextMenuEl.matches(":popover-open")) {
      try {
        contextMenuEl.hidePopover();
      } catch (_) {}
    }
    contextMenu = null;
    if (restaurarFoco && info?.triggerEl) {
      info.triggerEl.focus({ focusVisible: Boolean(info.viaKeyboard) });
    }
  }

  function ejecutarAccionMenu(callback) {
    const ruta = contextMenu?.ruta;
    cerrarMenuContextual({ restaurarFoco: false });
    if (ruta) callback?.(ruta);
  }

  function manejarToggleMenuContextual(e) {
    if (e.newState !== "closed") return;
    const info = contextMenu;
    contextMenu = null;
    if (restaurarFocoAlCerrar && info?.triggerEl) {
      info.triggerEl.focus({ focusVisible: Boolean(info.viaKeyboard) });
    }
    restaurarFocoAlCerrar = true;
  }

  function manejarKeydownMenuContextual(e) {
    if (!contextMenu) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      cerrarMenuContextual({ restaurarFoco: true });
      return;
    }
    if (e.key === "Tab") {
      cerrarMenuContextual({ restaurarFoco: false });
      return;
    }
    if (esSoporteFocusgroup()) return;
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) return;
    const items = [
      ...(contextMenuFocusgroupEl?.querySelectorAll("button:not(:disabled)") || []),
    ];
    if (items.length === 0) return;
    e.preventDefault();
    e.stopPropagation();
    const idx = items.indexOf(document.activeElement);
    let next = 0;
    if (e.key === "Home") next = 0;
    else if (e.key === "End") next = items.length - 1;
    else if (idx < 0) next = e.key === "ArrowUp" ? items.length - 1 : 0;
    else next = (idx + (e.key === "ArrowUp" ? -1 : 1) + items.length) % items.length;
    items[next]?.focus();
  }

  function manejarContextMenuLista(e) {
    e.preventDefault();
    if (!e.target?.closest?.("[data-context-item]") && contextMenu) {
      cerrarMenuContextual({ restaurarFoco: false });
    }
  }

  function manejarPointerDownVentana(e) {
    if (!contextMenu) return;
    if (contextMenuEl && contextMenuEl.contains(e.target)) return;
    // En Linux/WebKitGTK el menú contextual abre antes de soltar el botón derecho;
    // ignorar pointerdown derecho sobre filas del árbol para que oncontextmenu lo gestione.
    if (e.button === 2 && e.target?.closest?.("[data-context-item]")) return;
    cerrarMenuContextual({ restaurarFoco: false });
  }
</script>

<svelte:window
  onpointermove={manejarPointerMoveGlobal}
  onpointerup={manejarPointerUpGlobal}
  onpointercancel={finalizarEstadoArrastre}
  onpointerdown={manejarPointerDownVentana}
  onkeydown={manejarKeydownVentana}
  onblur={() => {
    finalizarEstadoArrastre();
    if (contextMenu) cerrarMenuContextual({ restaurarFoco: false });
  }}
/>

{#snippet renderArchivo(item, depth)}
  {@const isActive = archivoActual === item.ruta}
  {@const isContextOpen =
    contextMenu?.tipo === "archivo" && contextMenu?.ruta === item.ruta}
  {@const isDraggingThis =
    arrastreActivo?.tipo === "archivo" && arrastreActivo?.ruta === item.ruta}
  <div
    class="sidebar-file-row {isActive ? 'active' : ''} {isContextOpen ? 'is-context-open' : ''} {isDraggingThis ? 'is-dragging-source' : ''}"
    style="--tree-depth: {depth}"
    data-context-item="archivo"
    data-context-file={item.ruta}
  >
    <button
      type="button"
      class="sidebar-file-btn"
      draggable="false"
      ondragstart={(e) => e.preventDefault()}
      title="{item.ruta} · clic derecho para opciones o arrastra para mover"
      oncontextmenu={(e) => abrirMenuContextual(e, "archivo", item.ruta, e.currentTarget)}
      onpointerdown={(e) => {
        if (e.button === 0 && contextMenu) {
          cerrarMenuContextual({ restaurarFoco: false });
        }
        iniciarPosibleArrastre(e, {
          tipo: "archivo",
          ruta: item.ruta,
          nombre: item.nombre.replace(/\.xlsx$/i, ""),
        });
      }}
      onkeydown={(e) => {
        if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
          abrirMenuContextual(e, "archivo", item.ruta, e.currentTarget, true);
        }
      }}
      onclick={() => {
        if (bloquearClickTemporal) return;
        onSeleccionarArchivo(item.ruta);
        onCloseMobile?.();
      }}
    >
      <FileSpreadsheet size={15} strokeWidth={isActive ? 2.2 : 1.9} class="excel-icon" />
      <span class="file-name-truncate">{item.nombre.replace(/\.xlsx$/i, "")}</span>
    </button>
  </div>
{/snippet}

{#snippet renderCarpeta(nodo, depth, forzarAbierta)}
  {@const abierta = forzarAbierta || Boolean(carpetasExpandidas[nodo.ruta])}
  {@const folderColorStyle = estiloColorCarpeta(nodo.ruta)}
  {@const tieneActivoPlegado = !abierta && contieneArchivoActivo(nodo.ruta)}
  {@const isContextOpen =
    contextMenu?.tipo === "carpeta" && contextMenu?.ruta === nodo.ruta}
  {@const isDraggingThis =
    arrastreActivo?.tipo === "carpeta" && arrastreActivo?.ruta === nodo.ruta}
  {@const isDropHere = hoverDestino === nodo.ruta && arrastreActivo}
  {@const dropValido =
    isDropHere && esDestinoValido(arrastreActivo, nodo.ruta, hoverPosicion)}
  <div class="sidebar-folder-block">
    <div
      class="sidebar-folder-row {tieneActivoPlegado ? 'has-active-child' : ''} {isContextOpen ? 'is-context-open' : ''} {isDraggingThis ? 'is-dragging-source' : ''} {dropValido && hoverPosicion === 'dentro' ? 'is-drop-target' : ''} {dropValido && hoverPosicion === 'antes' ? 'is-drop-before' : ''} {dropValido && hoverPosicion === 'despues' ? 'is-drop-after' : ''}"
      style="--tree-depth: {depth}"
      data-drop-folder={nodo.ruta}
      data-context-item="carpeta"
      data-context-folder={nodo.ruta}
    >
      <button
        type="button"
        class="sidebar-folder-btn"
        draggable="false"
        ondragstart={(e) => e.preventDefault()}
        aria-expanded={abierta}
        title="{nodo.ruta}{tieneActivoPlegado ? ' (contiene la planilla activa)' : ''} · clic derecho para opciones o arrastra para reordenar"
        oncontextmenu={(e) => abrirMenuContextual(e, "carpeta", nodo.ruta, e.currentTarget)}
        onpointerdown={(e) => {
          if (e.button === 0 && contextMenu) {
            cerrarMenuContextual({ restaurarFoco: false });
          }
          iniciarPosibleArrastre(e, {
            tipo: "carpeta",
            ruta: nodo.ruta,
            nombre: nodo.nombre,
          });
        }}
        onkeydown={(e) => {
          if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
            abrirMenuContextual(e, "carpeta", nodo.ruta, e.currentTarget, true);
          }
        }}
        onclick={() => {
          if (bloquearClickTemporal) return;
          toggleCarpeta(nodo.ruta);
        }}
      >
        <span class="folder-chevron">
          {#if abierta}
            <ChevronDown size={13} strokeWidth={2.2} />
          {:else}
            <ChevronRight size={13} strokeWidth={2.2} />
          {/if}
        </span>
        {#if abierta}
          <FolderOpen size={14} strokeWidth={2} class="folder-icon" style={folderColorStyle} />
        {:else}
          <Folder size={14} strokeWidth={2} class="folder-icon" style={folderColorStyle} />
        {/if}
        <span class="file-name-truncate">{nodo.nombre}</span>
        {#if tieneActivoPlegado}
          <span class="folder-active-dot" aria-hidden="true"></span>
        {/if}
        {#if nodo.totalArchivos > 0}
          <span class="folder-count">{nodo.totalArchivos}</span>
        {/if}
      </button>
    </div>

    {#if abierta && (nodo.carpetas.length > 0 || nodo.archivos.length > 0)}
      <div
        class="sidebar-folder-children {dropValido && hoverPosicion === 'dentro' ? 'is-drop-target-children' : ''}"
        style="--tree-depth: {depth}"
        data-drop-folder={nodo.ruta}
        data-drop-only-inside="true"
      >
        {#each nodo.carpetas as subnodo (subnodo.ruta)}
          {@render renderCarpeta(subnodo, depth + 1, forzarAbierta)}
        {/each}
        {#each nodo.archivos as arch (arch.ruta)}
          {@render renderArchivo(arch, depth + 1)}
        {/each}
      </div>
    {/if}
  </div>
{/snippet}

{#if mobileOpen}
  <button
    type="button"
    class="sidebar-backdrop mobile-open"
    aria-label="Cerrar panel de planillas"
    onclick={onCloseMobile}
  ></button>
{/if}

<aside class="sidebar {mobileOpen ? 'mobile-open' : ''}">
  <div class="sidebar-brand">
    <div class="brand-id">
      <span class="brand-mark">
        <Scale size={14} strokeWidth={2.2} />
      </span>
      <span>Lokus Judicial</span>
    </div>
    <div class="sidebar-brand-actions">
      <button
        type="button"
        class="mini-icon-btn"
        onclick={() => onCrearCarpeta("")}
        title="Crear nueva carpeta"
        aria-label="Crear nueva carpeta"
      >
        <FolderPlus size={15} strokeWidth={2} />
      </button>
      <button
        type="button"
        class="mini-icon-btn"
        onclick={() => onCrearArchivo("")}
        title="Crear nuevo archivo Excel (Ctrl+N)"
        aria-label="Crear nuevo archivo Excel"
      >
        <Plus size={16} strokeWidth={2.2} />
      </button>
      {#if mobileOpen}
        <button
          type="button"
          class="mini-icon-btn sidebar-close-btn"
          onclick={onCloseMobile}
          title="Ocultar panel"
          aria-label="Ocultar panel"
        >
          <X size={15} />
        </button>
      {/if}
    </div>
  </div>

  <div class="sidebar-section-header">
    <span>Planillas (.xlsx)</span>
    <div class="sidebar-section-meta">
      <span>
        {#if arbolEstado.hayFiltro}
          {arbolEstado.raiz.totalArchivos} / {archivosDisponibles.length}
        {:else}
          {archivosDisponibles.length}
        {/if}
      </span>
      {#if carpetasDisponibles.length > 0 && !arbolEstado.hayFiltro}
        <button
          type="button"
          class="mini-icon-btn sidebar-collapse-btn"
          onclick={alternarTodasCarpetas}
          title={hayAlgunaCarpetaExpandida ? "Plegar todas las carpetas" : "Expandir todas las carpetas"}
          aria-label={hayAlgunaCarpetaExpandida ? "Plegar todas las carpetas" : "Expandir todas las carpetas"}
        >
          {#if hayAlgunaCarpetaExpandida}
            <ChevronsDownUp size={13} />
          {:else}
            <ChevronsUpDown size={13} />
          {/if}
        </button>
      {/if}
    </div>
  </div>

  {#if archivosDisponibles.length > 0 || carpetasDisponibles.length > 0}
    <search class="sidebar-search-wrap">
      <Search size={13} class="sidebar-search-icon" />
      <input
        type="search"
        class="sidebar-search-input"
        bind:value={busquedaPlanillas}
        placeholder="Buscar planilla o carpeta..."
        aria-label="Buscar planilla o carpeta"
        autocorrect="off"
        spellcheck="false"
      />
      {#if busquedaPlanillas}
        <button
          type="button"
          class="clear-search-btn"
          aria-label="Limpiar búsqueda"
          onclick={() => (busquedaPlanillas = "")}
        >
          <X size={12} />
        </button>
      {/if}
    </search>
  {/if}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="sidebar-files scroller"
    bind:this={filesContainerEl}
    data-drop-folder=""
    ondragstart={(e) => e.preventDefault()}
    oncontextmenu={manejarContextMenuLista}
  >
    {#if archivosDisponibles.length === 0 && carpetasDisponibles.length === 0}
      <div class="sidebar-empty">
        No hay planillas creadas.<br />
        Usa el botón <strong>+</strong> para crear la primera desde la plantilla de la Defensoría.
      </div>
    {:else if arbolEstado.raiz.carpetas.length === 0 && arbolEstado.raiz.archivos.length === 0}
      <div class="sidebar-empty">
        No hay planillas ni carpetas que coincidan con "{busquedaPlanillas.trim()}".
      </div>
    {:else}
      {#each arbolEstado.raiz.carpetas as nodo (nodo.ruta)}
        {@render renderCarpeta(nodo, 0, arbolEstado.hayFiltro)}
      {/each}
      {#each arbolEstado.raiz.archivos as arch (arch.ruta)}
        {@render renderArchivo(arch, 0)}
      {/each}
    {/if}
  </div>

  {#if arrastreActivo && arrastreActivo.carpetaOrigen !== ""}
    <div
      class="sidebar-root-dropzone {hoverDestino === '' ? 'is-drop-target' : ''}"
      data-drop-folder=""
    >
      <Folder size={13} />
      <span>Soltar aquí para mover a la carpeta principal</span>
    </div>
  {/if}

  <div class="sidebar-footer">
    {#if archivoActual}
      {@const pct =
        totales.num_registros > 0
          ? Math.round((totales.total_si / totales.num_registros) * 100)
          : 0}
      <div class="quota-box">
        <div class="quota-header">
          <span>Registros ({pct}% efectividad)</span>
          <strong>{totales.num_registros} / 100</strong>
        </div>
        <div class="quota-bar">
          <div
            class="quota-fill"
            style="width: {Math.min(100, totales.num_registros)}%"
          ></div>
        </div>
        <div class="quota-stats">
          <span><span class="stat-dot-si">●</span> {totales.total_si} realizadas</span>
          <span><span class="stat-dot-no">●</span> {totales.total_no} aplazadas</span>
        </div>
      </div>
    {/if}

    <div class="sidebar-bottom-bar">
      <button
        type="button"
        class="ai-config-trigger {aiStatus.configured ? '' : 'is-unconfigured'}"
        onclick={onAbrirConfigIa}
        title={aiStatus.configured ? "Configurar proveedor de IA" : "Falta configurar tu clave API de IA"}
      >
        <span class="led {aiStatus.configured ? 'on' : 'off'}"></span>
        <span class="ai-trigger-label">
          {#if aiStatus.configured}
            {aiStatus.provider === "gemini" ? "Gemini" : "OpenAI"} · {aiStatus.model}
          {:else}
            Configurar IA
          {/if}
        </span>
      </button>

      <div style="display: flex; gap: 3px; flex-shrink: 0">
        <button
          type="button"
          class="mini-icon-btn"
          onclick={onAbrirAtajos}
          title="Atajos de teclado (F1)"
          aria-label="Atajos de teclado"
        >
          <Keyboard size={14} />
        </button>
        <button
          type="button"
          class="mini-icon-btn"
          onclick={onToggleTema}
          title="Cambiar tema Azul Niebla / Oscuro (Ctrl+T)"
          aria-label="Cambiar tema"
        >
          {#if theme === "dark"}
            <Sun size={14} />
          {:else}
            <Moon size={14} />
          {/if}
        </button>
      </div>
    </div>
  </div>
</aside>

{#if arrastreActivo}
  <div
    class="tree-drag-ghost"
    style="left: {arrastreActivo.x + 12}px; top: {arrastreActivo.y + 12}px"
    aria-hidden="true"
  >
    {#if arrastreActivo.tipo === "carpeta"}
      <Folder
        size={13}
        class="folder-icon"
        style={estiloColorCarpeta(arrastreActivo.ruta)}
      />
    {:else}
      <FileSpreadsheet size={13} class="excel-icon" />
    {/if}
    <span>{arrastreActivo.nombre}</span>
  </div>
{/if}

{#snippet itemMenuContextual(Icono, etiqueta, callback, peligro = false)}
  <button
    type="button"
    class="folder-context-item {peligro ? 'is-danger' : ''}"
    onpointermove={(e) => {
      if (matchMedia("(hover: hover)").matches) {
        e.currentTarget.focus({ focusVisible: false });
      }
    }}
    onclick={() => ejecutarAccionMenu(callback)}
  >
    <span class="folder-context-icon" aria-hidden="true"><Icono size={14} /></span>
    <span>{etiqueta}</span>
  </button>
{/snippet}

<div class="context-menu-anchor" bind:this={contextAnchorEl} aria-hidden="true"></div>
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="folder-context-menu {contextMenu && !esSoportePopover() ? 'is-fallback-open' : ''}"
  bind:this={contextMenuEl}
  popover="manual"
  aria-label={contextMenu ? `Acciones para ${contextMenu.ruta}` : "Acciones"}
  ontoggle={manejarToggleMenuContextual}
  oncontextmenu={(e) => e.preventDefault()}
>
  {#if contextMenu}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="folder-context-list"
      bind:this={contextMenuFocusgroupEl}
      focusgroup="menu nomemory"
      tabindex="-1"
      aria-label={`Opciones de ${contextMenu.ruta}`}
      onkeydown={manejarKeydownMenuContextual}
    >
      {#if contextMenu.tipo === "carpeta"}
        {@render itemMenuContextual(Plus, "Nueva planilla aquí", onCrearArchivo)}
        {@render itemMenuContextual(FolderPlus, "Nueva subcarpeta", onCrearCarpeta)}
        {@render itemMenuContextual(Palette, "Cambiar color", onCambiarColorCarpeta)}
        {@render itemMenuContextual(Pencil, "Renombrar o mover", onRenombrarCarpeta)}
        <hr class="folder-context-sep" />
        {@render itemMenuContextual(Trash2, "Eliminar carpeta", onEliminarCarpeta, true)}
      {:else}
        {@render itemMenuContextual(Pencil, "Editar o renombrar planilla", onEditarArchivo)}
        {@render itemMenuContextual(FolderInput, "Mover a otra carpeta", onMoverArchivo)}
        {@render itemMenuContextual(UserCheck, "Asignar o editar defensor", onAbrirDefensorArchivo)}
        {@render itemMenuContextual(History, "Historial de respaldos", onAbrirRespaldos)}
        {@render itemMenuContextual(Download, "Exportar copia a Descargas", onDescargarArchivo)}
        <hr class="folder-context-sep" />
        {@render itemMenuContextual(Trash2, "Eliminar planilla", onEliminarArchivo, true)}
      {/if}
    </div>
  {/if}
</div>
