package {
    import flash.display.Sprite;
    import pkg.Greeter;

    public class Main extends Sprite {
        public function Main() {
            new Greeter().greet("a file");
        }
    }
}
