import { useCallback, useRef, useState } from 'react';
import { api } from '../../api/api';

// Cuánto se espera después del último toque antes de guardar. Así, si el
// mesero toca "Agua" tres veces seguidas, va UN solo envío con cantidad 3.
const ESPERA_MS = 350;

const redondear2 = (n) => Math.round(n * 100) / 100;

function conSubtotales(detalle, items) {
  return { ...detalle, items: items.map((i) => ({ ...i, subtotal: redondear2(i.cantidad * i.precio_unitario) })) };
}

/**
 * Pedido "optimista": cada toque se ve AL INSTANTE en pantalla y se guarda
 * en segundo plano (agrupado). Antes cada toque esperaba ~1 s al servidor
 * y los toques rápidos se perdían.
 *
 * Las líneas nuevas llevan un id temporal negativo hasta que el servidor
 * les asigna el real; si el mesero sigue tocando esa línea mientras se
 * guarda, el cambio se aplica a la línea real, no se pierde.
 */
export function usePedidoOptimista(pedidoId, onError) {
  const [detalle, setDetalle] = useState(null);
  const [guardando, setGuardando] = useState(false);
  const operaciones = useRef([]);
  const temporizador = useRef(null);
  const enCurso = useRef(null);
  const siguienteTemporal = useRef(-1);
  const temporalAReal = useRef(new Map());
  const idsConocidos = useRef(new Set());

  const recibirDelServidor = useCallback((d) => {
    idsConocidos.current = new Set(d.items.map((i) => i.id));
    return d;
  }, []);

  const cargar = useCallback(
    () => api.pedido(pedidoId).then((d) => setDetalle(recibirDelServidor(d))),
    [pedidoId, recibirDelServidor]
  );

  // Junta todo lo pendiente y lo manda en la menor cantidad de envíos.
  const procesarCola = useCallback(async () => {
    let ultimo = null;
    while (operaciones.current.length > 0) {
      const lote = operaciones.current;
      operaciones.current = [];

      const nuevos = new Map(); // idTemporal -> línea a crear
      const cambios = new Map(); // idReal -> { cantidad, nota }
      const quitar = new Set(); // idReal
      for (const op of lote) {
        if (op.tipo === 'agregar') {
          nuevos.set(op.id, { ...op.item });
          continue;
        }
        const id = op.id < 0 ? temporalAReal.current.get(op.id) ?? op.id : op.id;
        if (id < 0) {
          // Línea que todavía no existe en el servidor: se ajusta antes de crearla.
          const linea = nuevos.get(id);
          if (!linea) continue;
          if (op.tipo === 'quitar') nuevos.delete(id);
          else Object.assign(linea, { cantidad: op.cantidad, nota: op.nota ?? linea.nota });
          continue;
        }
        if (op.tipo === 'quitar') {
          quitar.add(id);
          cambios.delete(id);
        } else {
          const previo = cambios.get(id) || {};
          cambios.set(id, { cantidad: op.cantidad, nota: op.nota !== undefined ? op.nota : previo.nota });
        }
      }

      for (const id of quitar) ultimo = await api.pedidoQuitarItem(pedidoId, id);
      for (const [id, c] of cambios) ultimo = await api.pedidoCambiarCantidad(pedidoId, id, c.cantidad, c.nota);
      if (nuevos.size > 0) {
        const antes = new Set(idsConocidos.current);
        ultimo = await api.pedidoAgregarItems(pedidoId, [...nuevos.values()]);
        // Las líneas creadas llegan en el mismo orden en que se mandaron.
        const creadas = ultimo.items.filter((i) => !antes.has(i.id)).sort((a, b) => a.id - b.id);
        [...nuevos.keys()].forEach((temporal, n) => {
          if (creadas[n]) temporalAReal.current.set(temporal, creadas[n].id);
        });
      }
      if (ultimo) recibirDelServidor(ultimo);
    }
    return ultimo;
  }, [pedidoId, recibirDelServidor]);

  // Guarda lo pendiente (una vuelta; ver sincronizarTodo).
  const sincronizar = useCallback(() => {
    clearTimeout(temporizador.current);
    // Si ya se está guardando, esa misma vuelta recoge lo nuevo (procesarCola
    // repite mientras haya operaciones).
    if (enCurso.current) return enCurso.current;
    if (operaciones.current.length === 0) return Promise.resolve();
    setGuardando(true);
    enCurso.current = procesarCola()
      .then((ultimo) => {
        // Si mientras tanto hubo más toques, la pantalla ya los muestra; no
        // se pisa con una versión más vieja del servidor.
        if (ultimo && operaciones.current.length === 0) setDetalle(ultimo);
      })
      .catch(async (e) => {
        operaciones.current = [];
        onError?.(e.message);
        try {
          setDetalle(recibirDelServidor(await api.pedido(pedidoId)));
        } catch {
          // sin conexión: se queda lo que se ve
        }
      })
      .finally(() => {
        enCurso.current = null;
        setGuardando(false);
      });
    return enCurso.current;
  }, [pedidoId, procesarCola, recibirDelServidor, onError]);

  /** Espera hasta que TODO lo tocado quede guardado. */
  const sincronizarTodo = useCallback(async () => {
    while (enCurso.current || operaciones.current.length > 0) {
      await (enCurso.current || sincronizar());
    }
  }, [sincronizar]);

  const programar = useCallback(
    (op, cambiarLineas) => {
      setDetalle((d) => (d ? conSubtotales(d, cambiarLineas(d.items)) : d));
      operaciones.current.push(op);
      clearTimeout(temporizador.current);
      // Si justo se está guardando, se reintenta un poco después.
      const intentar = () => {
        if (enCurso.current) temporizador.current = setTimeout(intentar, ESPERA_MS);
        else sincronizar();
      };
      temporizador.current = setTimeout(intentar, ESPERA_MS);
    },
    [sincronizar]
  );

  /** linea: { producto_id, nombre_producto, precio_unitario, opciones, opcion_ids, nota, cantidad } */
  const agregar = useCallback(
    (linea) => {
      const id = siguienteTemporal.current--;
      programar(
        {
          tipo: 'agregar',
          id,
          item: { producto_id: linea.producto_id, cantidad: linea.cantidad, opcion_ids: linea.opcion_ids, nota: linea.nota },
        },
        (items) => [
          ...items,
          {
            id,
            producto_id: linea.producto_id,
            nombre_producto: linea.nombre_producto,
            opciones: linea.opciones || null,
            nota: linea.nota || null,
            cantidad: linea.cantidad,
            precio_unitario: linea.precio_unitario,
            estado: 'PENDIENTE',
          },
        ]
      );
    },
    [programar]
  );

  const cambiarCantidad = useCallback(
    (item, cantidad) =>
      programar({ tipo: 'cantidad', id: item.id, cantidad }, (items) =>
        items.map((i) => (i.id === item.id ? { ...i, cantidad } : i))
      ),
    [programar]
  );

  const cambiarNota = useCallback(
    (item, nota) =>
      programar({ tipo: 'nota', id: item.id, cantidad: item.cantidad, nota }, (items) =>
        items.map((i) => (i.id === item.id ? { ...i, nota: nota || null } : i))
      ),
    [programar]
  );

  const quitarPendiente = useCallback(
    (item) => programar({ tipo: 'quitar', id: item.id }, (items) => items.filter((i) => i.id !== item.id)),
    [programar]
  );

  /**
   * Trae lo último del servidor (p. ej. barra marcó algo listo), pero solo
   * si no hay cambios propios sin guardar: nunca pisa lo que el mozo tocó.
   */
  const refrescar = useCallback(() => {
    if (enCurso.current || operaciones.current.length > 0) return Promise.resolve();
    return api.pedido(pedidoId).then((d) => {
      if (!enCurso.current && operaciones.current.length === 0) setDetalle(recibirDelServidor(d));
    });
  }, [pedidoId, recibirDelServidor]);

  /** Reemplaza el pedido con una respuesta del servidor (enviar, mover, anular). */
  const aplicarServidor = useCallback((d) => setDetalle(recibirDelServidor(d)), [recibirDelServidor]);

  return {
    detalle,
    cargar,
    refrescar,
    guardando,
    sincronizar: sincronizarTodo,
    agregar,
    cambiarCantidad,
    cambiarNota,
    quitarPendiente,
    aplicarServidor,
  };
}
