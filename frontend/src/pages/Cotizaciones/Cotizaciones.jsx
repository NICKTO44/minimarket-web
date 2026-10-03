import { useCallback, useEffect, useState } from 'react';
import { FileText, Printer, ShoppingCart } from 'lucide-react';
import { api } from '../../api/api';
import { confirmar } from '../../utils/confirmar';
import { formatoCantidad } from '../../utils/medidas';
import { abreviaturaUnidad } from '../../utils/unidades';
import CotizacionImprimible from '../../components/CotizacionImprimible';
import { fechaCorta, numeroCotizacion } from '../../utils/formato';
import '../../components/PantallaModulo.css';

const FILTROS = [
  { valor: 'PENDIENTE', label: 'Pendientes' },
  { valor: 'VENDIDA', label: 'Vendidas' },
  { valor: '', label: 'Todas' },
];

/** Etiqueta de estado de una cotización. */
function EstadoCotizacion({ cotizacion }) {
  if (cotizacion.estado === 'VENDIDA') return <span className="pm-chip pm-chip-ok">Vendida</span>;
  if (cotizacion.estado === 'ANULADA') return <span className="pm-chip">Anulada</span>;
  if (cotizacion.vencida) return <span className="pm-chip pm-chip-mal">Vencida</span>;
  return <span className="pm-chip pm-chip-aviso">Pendiente</span>;
}

/**
 * Cotizaciones (módulo COTIZACIONES). Se crean desde el punto de venta
 * ("Guardar cotización"). Aquí se revisan, se imprimen, se anulan o se
 * cargan al punto de venta para venderlas al precio ofrecido.
 */
export default function Cotizaciones({ nombreTienda, direccion, telefono, ruc, onCargarEnVenta, onIrAVender }) {
  const [filtro, setFiltro] = useState('PENDIENTE');
  const [lista, setLista] = useState([]);
  const [cargando, setCargando] = useState(true);
  const [error, setError] = useState(null);
  const [abierta, setAbierta] = useState(null);
  const [ocupado, setOcupado] = useState(false);

  const cargar = useCallback(
    () =>
      api
        .cotizaciones(filtro)
        .then((datos) => {
          setLista(datos);
          setError(null);
        })
        .catch((e) => setError(e.message))
        .finally(() => setCargando(false)),
    [filtro]
  );

  useEffect(() => {
    cargar();
  }, [cargar]);

  const cambiarFiltro = (valor) => {
    if (valor === filtro) return;
    setCargando(true);
    setFiltro(valor);
  };

  const abrir = async (id) => {
    setError(null);
    try {
      setAbierta(await api.cotizacion(id));
    } catch (e) {
      setError(e.message);
    }
  };

  const anular = async () => {
    const ok = await confirmar({
      titulo: `¿Anular la cotización N° ${numeroCotizacion(abierta.numero)}?`,
      mensaje: 'Ya no se podrá vender desde esta cotización. No afecta stock ni caja.',
      textoConfirmar: 'Anular',
      icono: 'aviso',
    });
    if (!ok) return;
    setOcupado(true);
    try {
      await api.cotizacionAnular(abierta.id);
      setAbierta(null);
      cargar();
    } catch (e) {
      setError(e.message);
      setAbierta(null);
    } finally {
      setOcupado(false);
    }
  };

  return (
    <div className="pm-layout">
      <header className="pm-cabecera">
        <div>
          <h1>
            <FileText size={22} /> Cotizaciones
          </h1>
          <p className="pm-subtitulo">Se guardan desde el punto de venta. No tocan el stock ni la caja hasta que se venden.</p>
        </div>
        <div className="pm-acciones">
          <div className="pm-filtros" role="tablist">
            {FILTROS.map((f) => (
              <button key={f.valor} role="tab" aria-selected={filtro === f.valor} className={filtro === f.valor ? 'activo' : ''} onClick={() => cambiarFiltro(f.valor)}>
                {f.label}
              </button>
            ))}
          </div>
          <button className="pm-boton" onClick={onIrAVender}>
            <ShoppingCart size={15} /> Nueva cotización
          </button>
        </div>
      </header>

      {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}

      <div className="pm-tarjeta">
        {cargando ? (
          <p className="pm-vacio">Cargando...</p>
        ) : lista.length === 0 ? (
          <p className="pm-vacio">
            No hay cotizaciones {filtro === 'PENDIENTE' ? 'pendientes' : filtro === 'VENDIDA' ? 'vendidas' : ''}. Arma el pedido en el
            punto de venta y toca “Guardar cotización”.
          </p>
        ) : (
          lista.map((c) => (
            <button key={c.id} className="pm-fila" onClick={() => abrir(c.id)}>
              <span className="pm-fila-principal">
                <span className="pm-fila-titulo">
                  N° {numeroCotizacion(c.numero)} · {c.cliente_nombre || 'Sin cliente'} <EstadoCotizacion cotizacion={c} />
                </span>
                <span className="pm-fila-detalle">
                  {fechaCorta(c.fecha)} · {c.cantidad_items} {c.cantidad_items === 1 ? 'línea' : 'líneas'} · válida hasta {fechaCorta(c.vence)}
                  {c.folio_venta ? ` · venta ${c.folio_venta}` : ''}
                </span>
              </span>
              <span className="pm-fila-monto">
                <strong>S/ {c.total.toFixed(2)}</strong>
              </span>
            </button>
          ))
        )}
      </div>

      {abierta && (
        <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && setAbierta(null)}>
          <div className="pm-modal pm-modal-ancho" role="dialog" aria-modal="true" aria-label="Cotización">
            <div className="pm-modal-cabecera">
              <h2>
                Cotización N° {numeroCotizacion(abierta.numero)} <EstadoCotizacion cotizacion={abierta} />
              </h2>
              <p>
                {abierta.cliente_nombre || 'Sin cliente'}
                {abierta.cliente_documento ? ` · ${abierta.cliente_documento}` : ''} · {fechaCorta(abierta.fecha)} · válida hasta{' '}
                {fechaCorta(abierta.vence)}
              </p>
            </div>
            <div className="pm-modal-cuerpo">
              <table className="pm-lineas">
                <thead>
                  <tr>
                    <th>Producto</th>
                    <th className="pm-num">Cantidad</th>
                    <th className="pm-num">Precio</th>
                    <th className="pm-num">Importe</th>
                  </tr>
                </thead>
                <tbody>
                  {abierta.items.map((it, i) => (
                    <tr key={i}>
                      <td>
                        {it.nombre}
                        {it.detalle && <small>{it.detalle}</small>}
                      </td>
                      <td className="pm-num">
                        {formatoCantidad(it.cantidad)} {abreviaturaUnidad(it.unidad_medida)}
                      </td>
                      <td className="pm-num">S/ {it.precio_unitario.toFixed(2)}</td>
                      <td className="pm-num">S/ {it.total_linea.toFixed(2)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <div className="pm-total">
                <span>Total</span>
                <strong>S/ {abierta.total.toFixed(2)}</strong>
              </div>
              {abierta.notas && <p className="pm-subtitulo">Nota: {abierta.notas}</p>}
              {abierta.estado === 'PENDIENTE' && abierta.vencida && (
                <p className="pm-mensaje pm-mensaje-aviso" style={{ marginTop: 12 }}>
                  Ya pasó su fecha de validez. Puedes venderla igual: se cargará con los precios que se ofrecieron.
                </p>
              )}
            </div>
            <div className="pm-modal-pie">
              {abierta.estado === 'PENDIENTE' && (
                <button className="pm-boton-peligro" onClick={anular} disabled={ocupado}>
                  Anular
                </button>
              )}
              <button className="pm-boton-secundario" onClick={() => window.print()}>
                <Printer size={15} /> Imprimir
              </button>
              <button className="pm-boton-secundario" onClick={() => setAbierta(null)}>
                Cerrar
              </button>
              {abierta.estado === 'PENDIENTE' && (
                <button className="pm-boton" onClick={() => onCargarEnVenta(abierta)}>
                  <ShoppingCart size={15} /> Vender
                </button>
              )}
            </div>
          </div>
        </div>
      )}

      <CotizacionImprimible cotizacion={abierta} nombreTienda={nombreTienda} direccion={direccion} telefono={telefono} ruc={ruc} />
    </div>
  );
}
