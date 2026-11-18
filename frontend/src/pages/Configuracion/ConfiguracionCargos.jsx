import { useEffect, useState } from 'react';
import { api } from '../../api/api';
import BuscadorProducto from '../../components/BuscadorProducto';
import './ConfiguracionCargos.css';

/**
 * Configuración → Cargos rápidos (módulo CARGOS_RAPIDOS): qué productos salen
 * como botón al cobrar (vaso, bolsa, servicio...), cuántos van gratis en cada
 * venta y, si se quiere, el horario en que se agrega solo (servicio de
 * madrugada). El producto se crea antes en Productos, con su precio y stock.
 */
export default function ConfiguracionCargos({ onMensaje }) {
  const [productos, setProductos] = useState([]);
  const [filas, setFilas] = useState(null);
  const [guardadas, setGuardadas] = useState('[]');
  const [nuevo, setNuevo] = useState('');
  const [guardando, setGuardando] = useState(false);

  const aFilas = (lista) =>
    lista.map((c) => ({
      producto_id: c.producto_id,
      gratis: c.gratis_por_venta ? String(c.gratis_por_venta) : '',
      automatico: !!(c.auto_desde && c.auto_hasta),
      desde: c.auto_desde || '00:00',
      hasta: c.auto_hasta || '05:00',
    }));

  const aLista = (fs) =>
    fs.map((f) => ({
      producto_id: f.producto_id,
      gratis_por_venta: Number(String(f.gratis).replace(',', '.')) || 0,
      auto_desde: f.automatico ? f.desde : null,
      auto_hasta: f.automatico ? f.hasta : null,
    }));

  useEffect(() => {
    Promise.all([api.productos(), api.cargosRapidos()])
      .then(([p, c]) => {
        setProductos(p || []);
        setFilas(aFilas(c || []));
        setGuardadas(JSON.stringify(aLista(aFilas(c || []))));
      })
      .catch((e) => onMensaje({ tipo: 'error', texto: e.message }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!filas) return null;

  const nombreDe = (id) => productos.find((p) => p.id === id);
  const cambiar = (i, campo, valor) => setFilas((prev) => prev.map((f, n) => (n === i ? { ...f, [campo]: valor } : f)));
  const agregar = (id) => {
    if (!id) return;
    const pid = Number(id);
    if (!filas.some((f) => f.producto_id === pid)) {
      setFilas((prev) => [...prev, { producto_id: pid, gratis: '', automatico: false, desde: '00:00', hasta: '05:00' }]);
    }
    setNuevo('');
  };
  const sinCambios = JSON.stringify(aLista(filas)) === guardadas;
  const malo = filas.some((f) => {
    const g = Number(String(f.gratis).replace(',', '.'));
    return (f.gratis !== '' && !(g >= 0)) || (f.automatico && (!f.desde || !f.hasta || f.desde === f.hasta));
  });

  const guardar = async () => {
    setGuardando(true);
    try {
      const lista = await api.cargosRapidosGuardar(aLista(filas));
      setFilas(aFilas(lista));
      setGuardadas(JSON.stringify(aLista(aFilas(lista))));
      onMensaje({ tipo: 'exito', texto: 'Cargos rápidos guardados. Recarga el punto de venta para verlos.' });
    } catch (e) {
      onMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardando(false);
    }
  };

  return (
    <section className="cfg-card cc">
      <h3 className="cfg-subtitulo-seccion">Cargos rápidos</h3>
      <p className="cc-ayuda">
        Productos que salen como botón al cobrar (vaso, bolsa, servicio...). Créalos antes en <b>Productos</b> con su precio (y su
        stock, si lo llevan). Puedes poner cuántos van <b>gratis en cada venta</b> y un <b>horario</b> en que se agrega solo (por
        ejemplo, el servicio de madrugada de 00:00 a 05:00).
      </p>

      {filas.length > 0 && (
        <div className="cc-lista">
          {filas.map((f, i) => {
            const p = nombreDe(f.producto_id);
            return (
              <div key={f.producto_id} className="cc-fila">
                <div className="cc-producto">
                  <b>{p ? p.nombre : `Producto #${f.producto_id}`}</b>
                  {p && <small>S/ {Number(p.precio).toFixed(2)}</small>}
                </div>
                <label className="cc-campo">
                  Gratis por venta
                  <input
                    type="number"
                    min="0"
                    step="1"
                    placeholder="0"
                    value={f.gratis}
                    onChange={(e) => cambiar(i, 'gratis', e.target.value)}
                    aria-label={`Gratis por venta de ${p?.nombre || ''}`}
                  />
                </label>
                <label className="cc-auto">
                  <input type="checkbox" checked={f.automatico} onChange={(e) => cambiar(i, 'automatico', e.target.checked)} />
                  Agregar solo en un horario
                </label>
                {f.automatico && (
                  <div className="cc-horario">
                    de <input type="time" value={f.desde} onChange={(e) => cambiar(i, 'desde', e.target.value)} aria-label="Desde" /> a{' '}
                    <input type="time" value={f.hasta} onChange={(e) => cambiar(i, 'hasta', e.target.value)} aria-label="Hasta" />
                  </div>
                )}
                <button type="button" className="cc-quitar" onClick={() => setFilas((prev) => prev.filter((_, n) => n !== i))}>
                  Quitar
                </button>
              </div>
            );
          })}
        </div>
      )}

      <div className="cc-agregar">
        <BuscadorProducto productos={productos} valor={nuevo} onElegir={agregar} placeholder="Agregar producto (vaso, bolsa, servicio...)" />
      </div>

      <div className="cc-pie">
        <button type="button" className="cfg-boton-guardar" onClick={guardar} disabled={sinCambios || malo || guardando}>
          {guardando ? 'Guardando…' : 'Guardar cargos rápidos'}
        </button>
      </div>
    </section>
  );
}
