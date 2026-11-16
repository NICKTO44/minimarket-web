import { useEffect, useMemo, useRef, useState } from 'react';
import { api } from '../../api/api';
import BuscadorProducto from '../../components/BuscadorProducto';
import { ErrorDeArchivo, leerHoja } from '../../utils/hojas';
import { MOTIVOS_AJUSTE, cuerpoAjuste, leerConteo, lotesPorProducto, textoDiferencia } from '../../utils/ajusteInventario';
import { confirmar } from '../../utils/confirmar';

const claveDe = (l) => (l.lote ? `L${l.lote.lote_id}` : `P${l.producto.id}`);
const fecha = (f) => (f ? String(f).slice(0, 10).split('-').reverse().join('/') : '');

/**
 * Ajuste de inventario: después de contar, se corrige el stock (sube o
 * baja) con su motivo. A mano, producto por producto, o subiendo el Excel
 * de Productos → Exportar con lo contado.
 */
export default function AjusteInventario({ onAjustado }) {
  const [productos, setProductos] = useState([]);
  const [lotes, setLotes] = useState([]);
  const [cargando, setCargando] = useState(true);
  const [lineas, setLineas] = useState([]);
  const [motivo, setMotivo] = useState(MOTIVOS_AJUSTE[0]);
  const [mensaje, setMensaje] = useState(null);
  const [aviso, setAviso] = useState(null);
  const [guardando, setGuardando] = useState(false);

  // A mano
  const [elegido, setElegido] = useState('');
  const [loteElegido, setLoteElegido] = useState('');
  const [contado, setContado] = useState('');
  const contadoRef = useRef(null);
  const loteRef = useRef(null);

  const traer = () =>
    Promise.all([api.productos(), api.lotesVigentes()])
      .then(([p, l]) => {
        setProductos((p || []).filter((x) => x.controla_stock !== false));
        setLotes(l || []);
      })
      .catch((e) => setMensaje({ tipo: 'error', texto: e.message }))
      .finally(() => setCargando(false));
  useEffect(() => {
    traer();
  }, []);

  const porProducto = useMemo(() => lotesPorProducto(lotes), [lotes]);
  const producto = productos.find((p) => String(p.id) === String(elegido)) || null;
  const lotesDelProducto = producto?.lleva_vencimiento ? porProducto.get(producto.id) || [] : [];
  const lote = lotesDelProducto.find((l) => String(l.lote_id) === String(loteElegido)) || null;
  const sistema = producto ? (producto.lleva_vencimiento ? lote?.cantidad : Number(producto.stock) || 0) : null;

  const agregar = (linea) =>
    setLineas((prev) => [...prev.filter((x) => claveDe(x) !== claveDe(linea)), linea]);

  const agregarAMano = () => {
    const n = parseFloat(contado);
    if (!producto || !(n >= 0) || sistema == null) return;
    agregar({ producto, lote: producto.lleva_vencimiento ? lote : null, sistema, contado: Math.round(n * 1000) / 1000 });
    setElegido('');
    setLoteElegido('');
    setContado('');
  };

  const subirExcel = async (archivo) => {
    setMensaje(null);
    setAviso(null);
    try {
      const filas = await leerHoja(archivo);
      const r = leerConteo(filas, productos, lotes);
      if (r.error) {
        setMensaje({ tipo: 'error', texto: r.error });
        return;
      }
      r.lineas.forEach(agregar);
      const partes = [`${r.lineas.length} con diferencia`, `${r.sinCambio} ya cuadran`];
      setAviso({
        texto: `Excel leído (${r.usaContado ? 'columna Stock contado' : 'columna Stock'}): ${partes.join(', ')}.`,
        desconocidos: r.desconocidos,
        sinLote: r.sinLote,
      });
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e instanceof ErrorDeArchivo ? e.message : `No se pudo leer el archivo: ${e.message}` });
    }
  };

  const aplicar = async () => {
    const ok = await confirmar({
      titulo: '¿Aplicar el ajuste?',
      mensaje: `Se corregirá el stock de ${lineas.length} producto(s) o lote(s) con lo contado. Queda en el kárdex con tu usuario.`,
      textoConfirmar: 'Aplicar ajuste',
    });
    if (!ok) return;
    setGuardando(true);
    setMensaje(null);
    try {
      const r = await api.inventarioAjustar(cuerpoAjuste(lineas, motivo));
      setMensaje({ tipo: 'exito', texto: `Listo: ${r.ajustados.length} ajustado(s).${r.sin_cambio ? ` ${r.sin_cambio} ya cuadraban.` : ''}` });
      setLineas([]);
      setAviso(null);
      traer();
      onAjustado?.();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardando(false);
    }
  };

  if (cargando) return <p className="sl-cargando">Cargando...</p>;

  return (
    <div className="aj">
      <p className="sl-subtitulo">
        Después de contar, pon aquí lo que hay de verdad. El sistema corrige el stock (sube o baja) y lo anota en el kárdex con el
        motivo y tu usuario.
      </p>

      <div className="aj-formas">
        <section className="aj-caja">
          <h3>Producto por producto</h3>
          <div className="aj-mano">
            <BuscadorProducto
              productos={productos}
              valor={elegido}
              onElegir={(id) => {
                setElegido(id);
                const p = productos.find((x) => String(x.id) === String(id));
                const ls = p?.lleva_vencimiento ? porProducto.get(p.id) || [] : [];
                setLoteElegido(ls.length === 1 ? String(ls[0].lote_id) : '');
                // Se sigue con el lote (si hay que elegirlo) o con lo contado.
                if (id) setTimeout(() => (p?.lleva_vencimiento && ls.length > 1 ? loteRef.current : contadoRef.current)?.focus(), 0);
              }}
            />
            {producto?.lleva_vencimiento && (
              <select
                ref={loteRef}
                value={loteElegido}
                onChange={(e) => {
                  setLoteElegido(e.target.value);
                  if (e.target.value) setTimeout(() => contadoRef.current?.focus(), 0);
                }}
                aria-label="Lote"
              >
                <option value="">{lotesDelProducto.length ? 'Elige el lote…' : 'Sin lotes con stock'}</option>
                {lotesDelProducto.map((l) => (
                  <option key={l.lote_id} value={l.lote_id}>
                    {l.numero_lote ? `Lote ${l.numero_lote}` : 'Sin N.° de lote'} · vence {fecha(l.fecha_vencimiento)} · {l.cantidad}
                  </option>
                ))}
              </select>
            )}
            {sistema != null && <span className="aj-sistema">En sistema: <b>{sistema}</b></span>}
            <input
              ref={contadoRef}
              type="number"
              min="0"
              step="any"
              placeholder="Contado"
              aria-label="Cantidad contada"
              value={contado}
              onChange={(e) => setContado(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && agregarAMano()}
            />
            <button type="button" onClick={agregarAMano} disabled={!producto || sistema == null || !(parseFloat(contado) >= 0)}>
              + Agregar
            </button>
          </div>
          {producto?.lleva_vencimiento && lotesDelProducto.length === 0 && (
            <p className="aj-nota">Este producto no tiene lotes con stock. Un lote nuevo se agrega en Productos, con su vencimiento.</p>
          )}
        </section>

        <section className="aj-caja">
          <h3>Desde Excel</h3>
          <p className="aj-nota">
            En <b>Productos → ⬇ Exportar</b> descarga tu inventario, cambia la columna <b>Stock</b> por lo que contaste (o agrega una
            columna <b>Stock contado</b>) y súbelo aquí. Con lotes, una fila por lote.
          </p>
          <label className="aj-subir">
            📄 Subir Excel contado
            <input
              type="file"
              accept=".xlsx,.csv"
              onChange={(e) => {
                const f = e.target.files?.[0];
                e.target.value = '';
                if (f) subirExcel(f);
              }}
            />
          </label>
        </section>
      </div>

      {mensaje && <p className={`sl-mensaje sl-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}
      {aviso && (
        <div className="aj-aviso">
          <p>{aviso.texto}</p>
          {aviso.desconocidos.length > 0 && (
            <p>
              Códigos que no están en tu inventario ({aviso.desconocidos.length}): {aviso.desconocidos.slice(0, 15).join(', ')}
              {aviso.desconocidos.length > 15 ? '…' : ''}
            </p>
          )}
          {aviso.sinLote.length > 0 && (
            <p>
              Sin lote reconocido ({aviso.sinLote.length}): {aviso.sinLote.slice(0, 10).join('; ')}
              {aviso.sinLote.length > 10 ? '…' : ''}
            </p>
          )}
        </div>
      )}

      {lineas.length > 0 && (
        <section className="aj-revision">
          <table className="aj-tabla">
            <thead>
              <tr>
                <th>Producto</th>
                <th>Lote</th>
                <th>En sistema</th>
                <th>Contado</th>
                <th>Diferencia</th>
                <th aria-label="Quitar" />
              </tr>
            </thead>
            <tbody>
              {lineas.map((l) => {
                const d = l.contado - l.sistema;
                return (
                  <tr key={claveDe(l)}>
                    <td>
                      {l.producto.nombre}
                      {l.producto.codigo && <span className="aj-codigo">{l.producto.codigo}</span>}
                    </td>
                    <td>{l.lote ? l.lote.numero_lote || `vence ${fecha(l.lote.fecha_vencimiento)}` : '—'}</td>
                    <td>{l.sistema}</td>
                    <td>
                      <b>{l.contado}</b>
                    </td>
                    <td className={d > 0 ? 'aj-sube' : 'aj-baja'}>{textoDiferencia(d)}</td>
                    <td>
                      <button
                        type="button"
                        className="aj-quitar"
                        aria-label={`Quitar ${l.producto.nombre}`}
                        onClick={() => setLineas((prev) => prev.filter((x) => claveDe(x) !== claveDe(l)))}
                      >
                        ✕
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          <div className="aj-pie">
            <label>
              Motivo
              <select value={motivo} onChange={(e) => setMotivo(e.target.value)}>
                {MOTIVOS_AJUSTE.map((m) => (
                  <option key={m}>{m}</option>
                ))}
              </select>
            </label>
            <button type="button" className="aj-aplicar" onClick={aplicar} disabled={guardando}>
              {guardando ? 'Aplicando…' : `Aplicar ajuste (${lineas.length})`}
            </button>
          </div>
        </section>
      )}
    </div>
  );
}
