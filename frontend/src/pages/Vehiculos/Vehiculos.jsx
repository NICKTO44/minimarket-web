import { useCallback, useEffect, useState } from 'react';
import { Car, MessageCircle, Plus } from 'lucide-react';
import { api } from '../../api/api';
import { fechaCorta } from '../../utils/formato';
import { formatoCantidad } from '../../utils/medidas';
import { abreviaturaUnidad } from '../../utils/unidades';
import { enlaceWhatsapp, formatoPlaca, mensajeRecordatorio, nombreVehiculo, textoKm } from '../../utils/vehiculos';
import '../../components/PantallaModulo.css';

const VACIO = { placa: '', marca: '', modelo: '', anio: '', motor: '', color: '', notas: '', cliente: null };

/** "90,000 km o el 18/01/2027" */
function textoProximo(v) {
  return [v.proximo_km ? textoKm(v.proximo_km) : null, v.proxima_fecha ? fechaCorta(v.proxima_fecha) : null].filter(Boolean).join(' o el ');
}

/** Elegir el dueño: busca clientes por nombre o documento. */
function ElegirDueno({ cliente, onElegir }) {
  const [texto, setTexto] = useState('');
  const [resultados, setResultados] = useState([]);
  const consulta = texto.trim();
  useEffect(() => {
    if (consulta.length < 2) return undefined;
    const t = setTimeout(() => {
      api
        .clientesBuscar(consulta)
        .then((r) => setResultados(r.slice(0, 6)))
        .catch(() => setResultados([]));
    }, 300);
    return () => clearTimeout(t);
  }, [consulta]);

  if (cliente) {
    return (
      <div className="pm-pares">
        <span>
          {cliente.nombre_razon_social}
          {cliente.numero_documento ? ` · ${cliente.numero_documento}` : ''}
        </span>
        <button type="button" className="pm-boton-secundario" onClick={() => onElegir(null)}>
          Cambiar
        </button>
      </div>
    );
  }
  return (
    <>
      <input value={texto} placeholder="Nombre o DNI/RUC del dueño (opcional)" onChange={(e) => setTexto(e.target.value)} />
      {(consulta.length < 2 ? [] : resultados).map((c) => (
        <button
          type="button"
          key={c.id}
          className="pm-fila"
          onClick={() => {
            onElegir(c);
            setTexto('');
          }}
        >
          <span className="pm-fila-principal">
            <span className="pm-fila-titulo">{c.nombre_razon_social}</span>
            <span className="pm-fila-detalle" style={{ display: 'block' }}>{c.numero_documento || 'Sin documento'}</span>
          </span>
        </button>
      ))}
    </>
  );
}

/**
 * Vehículos (módulo "Vehículos y kilometraje", lubricentro): los vehículos
 * de los clientes por placa, su historial de servicios y los recordatorios
 * del próximo servicio por WhatsApp. Los vehículos también se registran
 * solos al vender con placa.
 */
export default function Vehiculos({ nombreTienda }) {
  const [vista, setVista] = useState('LISTA');
  const [buscar, setBuscar] = useState('');
  const [lista, setLista] = useState([]);
  const [cargando, setCargando] = useState(true);
  const [error, setError] = useState(null);
  const [aviso, setAviso] = useState(null);
  const [dias, setDias] = useState(7);
  const [recordatorios, setRecordatorios] = useState([]);
  // Ficha abierta: { id (null = nuevo), form, historial }.
  const [ficha, setFicha] = useState(null);
  const [guardando, setGuardando] = useState(false);

  const cargar = useCallback(() => {
    setCargando(true);
    const pedido = vista === 'LISTA' ? api.vehiculos(buscar.trim()) : api.vehiculosRecordatorios(dias);
    pedido
      .then((datos) => {
        if (vista === 'LISTA') setLista(datos);
        else setRecordatorios(datos);
        setError(null);
      })
      .catch((e) => setError(e.message))
      .finally(() => setCargando(false));
  }, [vista, buscar, dias]);

  useEffect(() => {
    const t = setTimeout(cargar, buscar ? 300 : 0);
    return () => clearTimeout(t);
  }, [cargar, buscar]);

  const abrir = async (v) => {
    setError(null);
    setFicha({
      id: v.id,
      form: {
        placa: v.placa,
        marca: v.marca || '',
        modelo: v.modelo || '',
        anio: v.anio ? String(v.anio) : '',
        motor: v.motor || '',
        color: v.color || '',
        notas: v.notas || '',
        cliente: v.cliente_id ? { id: v.cliente_id, nombre_razon_social: v.cliente_nombre, numero_documento: v.cliente_documento } : null,
      },
      vehiculo: v,
      historial: null,
    });
    try {
      const historial = await api.vehiculoHistorial(v.id);
      setFicha((f) => (f && f.id === v.id ? { ...f, historial } : f));
    } catch (e) {
      setFicha((f) => (f && f.id === v.id ? { ...f, historial: [], errorHistorial: e.message } : f));
    }
  };

  const cambiar = (campo, valor) => setFicha((f) => ({ ...f, form: { ...f.form, [campo]: valor } }));

  const guardar = async () => {
    const f = ficha.form;
    if (!formatoPlaca(f.placa)) {
      setError('Revisa la placa (por ejemplo ABC-123).');
      return;
    }
    const datos = {
      placa: f.placa,
      cliente_id: f.cliente?.id || null,
      marca: f.marca.trim() || null,
      modelo: f.modelo.trim() || null,
      anio: f.anio ? Number(f.anio) : null,
      motor: f.motor.trim() || null,
      color: f.color.trim() || null,
      notas: f.notas.trim() || null,
    };
    setGuardando(true);
    setError(null);
    try {
      const v = ficha.id ? await api.vehiculoActualizar(ficha.id, datos) : await api.vehiculoCrear(datos);
      setFicha(null);
      setAviso(`Vehículo ${v.placa} guardado.`);
      cargar();
    } catch (e) {
      setError(e.message);
    } finally {
      setGuardando(false);
    }
  };

  return (
    <div className="pm-layout">
      <header className="pm-cabecera">
        <div>
          <h1>
            <Car size={22} /> Vehículos
          </h1>
          <p className="pm-subtitulo">Se registran solos al vender con placa. Aquí ves su historial y a quién le toca su próximo servicio.</p>
        </div>
        <div className="pm-acciones">
          <div className="pm-filtros" role="tablist">
            {[
              ['LISTA', 'Vehículos'],
              ['RECORDATORIOS', 'Recordatorios'],
            ].map(([valor, label]) => (
              <button key={valor} role="tab" aria-selected={vista === valor} className={vista === valor ? 'activo' : ''} onClick={() => setVista(valor)}>
                {label}
              </button>
            ))}
          </div>
          <button className="pm-boton" onClick={() => setFicha({ id: null, form: { ...VACIO }, historial: [] })}>
            <Plus size={15} /> Nuevo vehículo
          </button>
        </div>
      </header>

      {error && !ficha && <p className="pm-mensaje pm-mensaje-error">{error}</p>}
      {aviso && <p className="pm-mensaje pm-mensaje-exito">{aviso}</p>}

      {vista === 'LISTA' && (
        <div className="pm-campo" style={{ marginBottom: 12 }}>
          <input value={buscar} placeholder="Buscar por placa, marca, modelo o dueño" onChange={(e) => setBuscar(e.target.value)} />
        </div>
      )}
      {vista === 'RECORDATORIOS' && (
        <div className="pm-filtros" role="tablist" style={{ marginBottom: 12 }}>
          {[7, 15, 30].map((d) => (
            <button key={d} role="tab" aria-selected={dias === d} className={dias === d ? 'activo' : ''} onClick={() => setDias(d)}>
              Próximos {d} días
            </button>
          ))}
        </div>
      )}

      <div className="pm-tarjeta">
        {cargando ? (
          <p className="pm-vacio">Cargando...</p>
        ) : vista === 'LISTA' ? (
          lista.length === 0 ? (
            <p className="pm-vacio">{buscar ? 'No hay vehículos con esa búsqueda.' : 'Aún no hay vehículos. Se registran al vender con placa.'}</p>
          ) : (
            lista.map((v) => (
              <button key={v.id} className="pm-fila" onClick={() => abrir(v)}>
                <span className="pm-fila-principal">
                  <span className="pm-fila-titulo">
                    {v.placa} · {nombreVehiculo(v) || 'Sin datos del vehículo'}
                  </span>
                  <span className="pm-fila-detalle" style={{ display: 'block' }}>
                    {v.cliente_nombre || 'Sin dueño'}
                    {v.km_ultimo != null && ` · ${textoKm(v.km_ultimo)}`}
                    {v.ultimo_servicio && ` · último servicio ${fechaCorta(v.ultimo_servicio)}`}
                    {textoProximo(v) && ` · próximo: ${textoProximo(v)}`}
                  </span>
                </span>
                <span className="pm-fila-monto">
                  <strong>{v.servicios}</strong>
                  <small>{v.servicios === 1 ? 'servicio' : 'servicios'}</small>
                </span>
              </button>
            ))
          )
        ) : recordatorios.length === 0 ? (
          <p className="pm-vacio">Nadie tiene su próximo servicio en estos días.</p>
        ) : (
          recordatorios.map((v) => {
            const enlace = enlaceWhatsapp(v.cliente_telefono, mensajeRecordatorio(v, nombreTienda));
            const vencido = v.proxima_fecha && v.proxima_fecha < new Date(Date.now() - 5 * 3600 * 1000).toISOString().slice(0, 10);
            return (
              <div key={v.id} className="pm-fila">
                <span className="pm-fila-principal">
                  <span className="pm-fila-titulo">
                    {v.placa} · {v.cliente_nombre || 'Sin dueño'}{' '}
                    {vencido ? <span className="pm-chip pm-chip-mal">Pasó la fecha</span> : <span className="pm-chip pm-chip-aviso">Pronto</span>}
                  </span>
                  <span className="pm-fila-detalle" style={{ display: 'block' }}>
                    {nombreVehiculo(v)} · próximo: {textoProximo(v)}
                    {v.km_ultimo != null && ` · último: ${textoKm(v.km_ultimo)}`}
                  </span>
                </span>
                {enlace ? (
                  <a className="pm-boton" href={enlace} target="_blank" rel="noreferrer">
                    <MessageCircle size={15} /> WhatsApp
                  </a>
                ) : (
                  <span className="pm-fila-detalle" style={{ display: 'block' }}>Sin celular</span>
                )}
              </div>
            );
          })
        )}
      </div>

      {ficha && (
        <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && setFicha(null)}>
          <div className="pm-modal pm-modal-ancho" role="dialog" aria-modal="true" aria-label="Vehículo">
            <div className="pm-modal-cabecera">
              <h2>{ficha.id ? `Vehículo ${ficha.vehiculo?.placa}` : 'Nuevo vehículo'}</h2>
              {ficha.vehiculo && (
                <p>
                  {ficha.vehiculo.km_ultimo != null ? `Último kilometraje: ${textoKm(ficha.vehiculo.km_ultimo)}` : 'Sin kilometraje anotado'}
                  {textoProximo(ficha.vehiculo) && ` · próximo servicio: ${textoProximo(ficha.vehiculo)}`}
                </p>
              )}
            </div>
            <div className="pm-modal-cuerpo">
              {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}
              <div className="pm-campos">
                <label className="pm-campo">
                  <span>Placa</span>
                  <input value={ficha.form.placa} maxLength={10} onChange={(e) => cambiar('placa', e.target.value.toUpperCase())} />
                </label>
                <label className="pm-campo">
                  <span>Marca</span>
                  <input value={ficha.form.marca} maxLength={40} onChange={(e) => cambiar('marca', e.target.value)} />
                </label>
                <label className="pm-campo">
                  <span>Modelo</span>
                  <input value={ficha.form.modelo} maxLength={60} onChange={(e) => cambiar('modelo', e.target.value)} />
                </label>
                <label className="pm-campo">
                  <span>Año</span>
                  <input
                    inputMode="numeric"
                    value={ficha.form.anio}
                    onChange={(e) => cambiar('anio', e.target.value.replace(/[^\d]/g, '').slice(0, 4))}
                  />
                </label>
                <label className="pm-campo">
                  <span>Motor</span>
                  <input value={ficha.form.motor} maxLength={40} placeholder="2.5 D-4D" onChange={(e) => cambiar('motor', e.target.value)} />
                </label>
                <label className="pm-campo">
                  <span>Color</span>
                  <input value={ficha.form.color} maxLength={30} onChange={(e) => cambiar('color', e.target.value)} />
                </label>
                <div className="pm-campo pm-campo-ancho">
                  <span>Dueño</span>
                  <ElegirDueno cliente={ficha.form.cliente} onElegir={(c) => cambiar('cliente', c)} />
                </div>
                <label className="pm-campo pm-campo-ancho">
                  <span>Notas</span>
                  <textarea rows={2} value={ficha.form.notas} maxLength={300} onChange={(e) => cambiar('notas', e.target.value)} />
                </label>
              </div>

              {ficha.id && (
                <>
                  <p className="pm-seccion">Historial</p>
                  {ficha.historial === null ? (
                    <p className="pm-vacio">Cargando...</p>
                  ) : ficha.historial.length === 0 ? (
                    <p className="pm-vacio">{ficha.errorHistorial || 'Todavía no tiene servicios.'}</p>
                  ) : (
                    ficha.historial.map((s) => (
                      <div key={s.venta_id} className="pm-fila">
                        <span className="pm-fila-principal">
                          <span className="pm-fila-titulo">
                            {fechaCorta(s.fecha)} · {s.folio} {s.anulada && <span className="pm-chip">Anulada</span>}
                          </span>
                          <span className="pm-fila-detalle" style={{ display: 'block', whiteSpace: 'normal' }}>
                            {s.km != null && `${textoKm(s.km)} · `}
                            {s.items.map((i) => `${formatoCantidad(i.cantidad)} ${abreviaturaUnidad(i.unidad_medida)} ${i.nombre}`).join(', ')}
                          </span>
                          {(s.proximo_km || s.proxima_fecha) && (
                            <span className="pm-fila-detalle" style={{ display: 'block' }}>Próximo: {textoProximo(s)}</span>
                          )}
                        </span>
                        <span className="pm-fila-monto">
                          <strong>S/ {s.total.toFixed(2)}</strong>
                        </span>
                      </div>
                    ))
                  )}
                </>
              )}
            </div>
            <div className="pm-modal-pie">
              <button
                className="pm-boton-secundario"
                onClick={() => {
                  setFicha(null);
                  setError(null);
                }}
              >
                Cerrar
              </button>
              <button className="pm-boton" onClick={guardar} disabled={guardando}>
                {guardando ? 'Guardando...' : 'Guardar'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
