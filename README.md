# Dynamic Island para o painel do COSMIC

Applet no estilo da Dynamic Island do iPhone e das Live Activities do macOS: uma ilha preta
no painel que mostra o que está acontecendo e, ao clicar, se abre nas atividades ao vivo.

![A ilha no painel, com a música tocando e um timer ao lado](assets/panel.png)

No painel, a ilha mostra a atividade mais importante:

- **mídia**: a capa, o título e barras que dançam na cor da capa enquanto toca (pausadas,
  elas descansam embaixo); funciona com qualquer player MPRIS: Spotify, navegadores,
  YouTube, VLC, mpv, Rhythmbox… Um player pausado sai da ilha depois de 5 minutos;
- **timer**: a contagem regressiva em laranja. Com música tocando, o timer vai para uma
  bolha ao lado da ilha, como no iPhone;
- **carregando**: ao ligar o carregador, a ilha mostra por um instante "Carregando" e a
  porcentagem em verde;
- **acabou o tempo**: quando o timer termina a ilha avisa, e chega uma notificação.

Sem nada acontecendo, ela fica uma pílula preta pequena, como a câmera do iPhone.
O botão do meio do mouse sobre a ilha toca ou pausa a música.

Clicar abre a ilha, que cresce a partir do painel com uma mola:

![A ilha aberta, com o player e o timer](assets/popup.png)

- o **player**: a capa grande (clicar nela traz a janela do player), o título, o artista,
  o nome do player, as barras, o progresso, que dá para arrastar, com o tempo que passou e
  o que falta, e os botões de anterior, tocar/pausar e próxima. O cartão brilha na cor da
  capa. Com mais de um player aberto, aparecem os nomes deles para escolher qual mostrar
  (quem começa a tocar passa à frente);
- o **timer**: botões de 1, 5, 10 e 25 minutos para começar um; rodando, mostra o tempo
  que falta, a barra de progresso, +1:00, pausar e cancelar;
- a **bateria**, enquanto carrega: a porcentagem e quanto falta para encher.

O texto segue o idioma do sistema: português ou inglês.

## Instalar

Precisa do Rust (1.85 ou mais novo) e das bibliotecas de desenvolvimento do libcosmic
(`libxkbcommon`, `wayland`, `pkg-config`…). Depois:

```sh
./install.sh            # em ~/.local
sudo ./install.sh /usr  # para todos os usuários
```

E adicione **Dynamic Island** em Configurações > Área de trabalho > Painel > Applets,
de preferência no centro do painel de cima.

## Desenvolvimento

`COSMIC_ISLAND_PREVIEW=popup` abre a ilha sozinha ao iniciar e `=timer` começa um
timer de 1 minuto, para tirar capturas de tela sem clicar no painel
(`COSMIC_ISLAND_PREVIEW=timer,popup` faz as duas coisas).

## Licença

GPL-3.0-only.
